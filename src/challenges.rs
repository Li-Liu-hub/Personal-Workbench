use crate::study::{Card, Rubric};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
pub const ID: &str = "project:checkout:1";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub files: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub compiled: bool,
    pub passed: usize,
    pub total: usize,
    pub output: String,
    pub elapsed_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Saved {
    pub card_id: String,
    pub source: Source,
    pub report: Option<Report>,
}
const TEST: &str = include_str!("../assets/challenges/checkout/RegressionTest.java");
pub fn original() -> Source {
    Source {
        files: [
            (
                "Order.java",
                include_str!("../assets/challenges/checkout/Order.java"),
            ),
            (
                "Catalog.java",
                include_str!("../assets/challenges/checkout/Catalog.java"),
            ),
            (
                "Inventory.java",
                include_str!("../assets/challenges/checkout/Inventory.java"),
            ),
            (
                "OrderRepository.java",
                include_str!("../assets/challenges/checkout/OrderRepository.java"),
            ),
            (
                "OrderService.java",
                include_str!("../assets/challenges/checkout/OrderService.java"),
            ),
        ]
        .into_iter()
        .map(|(name, text)| (format!("src/main/java/training/{name}"), text.into()))
        .collect(),
    }
}
pub fn card() -> Card {
    Card{knowledge_ids:vec![],tags:vec!["项目实战".into(),"库存".into(),"幂等".into(),"并发".into()],id:ID.into(),kind:"engineering".into(),title:"修复订单服务：库存、幂等与并发".into(),topic:"项目修 Bug".into(),
        prompt:"一个 Java 17 多文件订单服务出现了重复请求重复扣库存、非法数量被接受、失败后库存变化和并发错误。导出项目，在自己的编辑器修复，保持公共方法签名不变；导入修复后的 ZIP，再运行随题提供的 9 个回归场景。你可以修改主源码，验证始终使用原始测试。".into(),
        answer:"先校验 requestId、数量和商品，再检查幂等键；相同请求返回原订单，不同内容复用同一键应拒绝且不改库存。在扣库存前检查可用数量。将检查、扣减与订单保存放入同一个并发一致的边界（例如此单实例示例中对 place 同步）。错误路径不能留下库存改动。生产系统还需数据库事务、原子库存更新和持久化幂等约束；本练习为内存 Java 示例。".into(),
        rubric:[("校验非法请求且失败不改变库存",25),("正确处理幂等重试和请求编号冲突",25),("库存与订单在并发下保持一致",25),("保持金额计算和现有接口正确，结合测试检查边界",25)].into_iter().enumerate().map(|(i,(text,points))|Rubric{id:format!("r{}",i+1),text:text.into(),points}).collect(),
        source:"https://docs.oracle.com/javase/17/docs/api/java.base/java/util/concurrent/package-summary.html".into(),phonetic:String::new(),sql:None,origin:None,project:Some("checkout".into()),coding:None,training:None}
}
pub fn export(source: &Source, path: &Path) -> Result<()> {
    let parent = path.parent().ok_or("导出目录不可用")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut zip = zip::ZipWriter::new(temp.as_file_mut());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, text) in &source.files {
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            zip.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        }
        zip.start_file("src/test/java/training/RegressionTest.java", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(TEST.as_bytes()).map_err(|e| e.to_string())?;
        zip.start_file("README.md", options)
            .map_err(|e| e.to_string())?;
        zip.write_all(b"# Checkout repair challenge\n\nRequires JDK 17+. Fix main sources without changing public signatures.\nThe app uses its original regression tests when you import the repaired ZIP.\n\nCompile main and test Java sources with javac -encoding UTF-8 -d out.\nRun: java -cp out training.RegressionTest\n").map_err(|e|e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
    }
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn import(path: &Path) -> Result<Source> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
        return Err("修复项目 ZIP 请控制在 8 MiB 内".into());
    }
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > 100 {
        return Err("项目文件过多，请只打包这道题的源码".into());
    }
    let expected = original();
    let mut files = BTreeMap::new();
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().replace('\\', "/");
        let Some(key) = expected
            .files
            .keys()
            .find(|key| name == key.as_str() || name.ends_with(&format!("/{key}")))
            .cloned()
        else {
            continue;
        };
        if files.contains_key(&key) {
            return Err(format!("项目中有重复源码：{key}"));
        }
        let mut text = String::new();
        file.take(128 * 1024 + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() > 128 * 1024 {
            return Err("单份源码过大".into());
        }
        files.insert(key, text);
    }
    if files.len() != expected.files.len() {
        return Err("ZIP 需要包含本题全部 5 份主源码，路径为 src/main/java/training/…".into());
    }
    Ok(Source { files })
}
pub fn answer(source: &Source) -> String {
    source
        .files
        .iter()
        .map(|(path, text)| format!("// {path}\n{text}"))
        .collect::<Vec<_>>()
        .join("\n")
}
fn process(
    program: &str,
    args: &[String],
    directory: &Path,
    timeout: Duration,
) -> Result<(bool, String)> {
    let output = directory.join(format!("{program}-output.txt"));
    let file = fs::File::create(&output).map_err(|e| e.to_string())?;
    let mut command = Command::new(program);
    command
        .current_dir(directory)
        .args(args)
        .stdout(Stdio::from(file.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(file));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("无法启动 {program}，需要安装 JDK 17 并加入 PATH：{e}"))?;
    #[cfg(windows)]
    let _job = match crate::local_ai::ProcessJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.to_string());
        }
    };
    let started = Instant::now();
    let success = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status.success();
        }
        if fs::metadata(&output).is_ok_and(|m| m.len() > 65_536) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("项目输出超过 64 KiB，已停止子进程".into());
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err("项目测试超时，已停止子进程".into());
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let mut bytes = Vec::new();
    fs::File::open(output)
        .map_err(|e| e.to_string())?
        .take(65_536)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok((success, String::from_utf8_lossy(&bytes).into_owned()))
}
pub fn run(source: &Source) -> Report {
    let started = Instant::now();
    let mut report = Report {
        total: 9,
        ..Default::default()
    };
    let outcome = (|| -> Result<()> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let output = dir.path().join("out");
        fs::create_dir(&output).map_err(|e| e.to_string())?;
        let mut args = vec![
            "-encoding".into(),
            "UTF-8".into(),
            "-d".into(),
            output.to_string_lossy().into_owned(),
        ];
        let expected = original();
        for key in expected.files.keys() {
            let text = source.files.get(key).ok_or("缺少主源码")?;
            let name = Path::new(key).file_name().unwrap();
            let path = dir.path().join(name);
            fs::write(&path, text).map_err(|e| e.to_string())?;
            args.push(path.to_string_lossy().into_owned());
        }
        let test = dir.path().join("RegressionTest.java");
        fs::write(&test, TEST).map_err(|e| e.to_string())?;
        args.push(test.to_string_lossy().into_owned());
        let (compiled, text) = process("javac", &args, dir.path(), Duration::from_secs(25))?;
        report.compiled = compiled;
        report.output = text;
        if compiled {
            let (_, text) = process(
                "java",
                &[
                    "-Xmx128m".into(),
                    "-cp".into(),
                    output.to_string_lossy().into_owned(),
                    "training.RegressionTest".into(),
                ],
                dir.path(),
                Duration::from_secs(8),
            )?;
            report.passed = text
                .lines()
                .filter(|line| line.starts_with("PASS "))
                .count()
                .min(9);
            report.output = text;
        }
        Ok(())
    })();
    if let Err(error) = outcome {
        report.output.push_str(&format!("\n{error}"));
    }
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    report
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_roundtrip_excludes_reference_solution_and_keeps_sources() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.zip");
        let source = original();
        export(&source, &path).unwrap();
        assert_eq!(source.files, import(&path).unwrap().files);
        let zip = zip::ZipArchive::new(fs::File::open(&path).unwrap()).unwrap();
        assert!(!zip.file_names().any(|name| name.contains("Reference")));
    }
    #[test]
    #[ignore = "requires a local JDK 17 or newer"]
    fn java_challenge_fails_before_and_passes_after_reference_fix() {
        let mut source = original();
        let before = run(&source);
        println!(
            "Original project: {}/{} passed",
            before.passed, before.total
        );
        assert!(before.compiled && before.passed < 9, "{before:?}");
        source.files.insert(
            "src/main/java/training/OrderService.java".into(),
            include_str!("../assets/challenges/checkout/ReferenceOrderService.java").into(),
        );
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("fixed.zip");
        export(&source, &zip).unwrap();
        let restored = import(&zip).unwrap();
        let after = run(&restored);
        println!(
            "Imported fixed project: {}/{} passed",
            after.passed, after.total
        );
        assert!(after.compiled && after.passed == 9, "{after:?}");
    }
}
