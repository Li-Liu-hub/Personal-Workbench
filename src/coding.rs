use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub input: String,
    pub expected: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lab {
    pub language: String,
    pub starter: String,
    pub cases: Vec<Case>,
}
impl Lab {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.language.as_str(), "Java" | "C")
            || self.starter.len() > 65_536
            || self.cases.is_empty()
            || self.cases.len() > 12
            || self
                .cases
                .iter()
                .any(|c| c.input.len() > 8192 || c.expected.len() > 8192)
        {
            return Err("编码题需选择 Java 或 C，源码不超过 64 KiB，提供 1～12 组小型样例".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseReport {
    pub input: String,
    pub expected: String,
    pub actual: String,
    pub error: String,
    pub passed: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub compiled: bool,
    #[serde(default)]
    pub total: usize,
    pub cases: Vec<CaseReport>,
    pub output: String,
    pub elapsed_ms: u64,
}
#[derive(Clone, Debug)]
pub struct Run {
    pub source: String,
    pub report: Report,
}
impl Report {
    pub fn passed(&self) -> usize {
        if !self.compiled {
            return 0;
        }
        self.cases.iter().filter(|c| c.passed).count()
    }
    pub fn grade(&self, card: &crate::study::Card, source: &str) -> crate::study::Grade {
        let mut grade = crate::study::Grade::unknown(card);
        let total = card.coding.as_ref().map_or(0, |l| l.cases.len());
        // Recheck saved evidence against this task rather than trusting a pass flag.
        let passed = card.coding.as_ref().map_or(0, |lab| {
            if !self.compiled || self.total != total || self.cases.len() > total {
                return 0;
            }
            self.cases
                .iter()
                .zip(&lab.cases)
                .filter(|(report, case)| {
                    report.passed
                        && report.input == case.input
                        && report.expected == case.expected
                        && normalized(&report.actual) == normalized(&case.expected)
                })
                .count()
        });
        for check in &mut grade.checks {
            let points = card
                .rubric
                .iter()
                .find(|r| r.id == check.id)
                .unwrap()
                .points;
            check.earned = (usize::from(points) * passed)
                .checked_div(total)
                .unwrap_or(0) as u8;
            check.reason = format!(
                "真实编译={}；通过 {}/{} 组样例。仅按提供的样例判分。",
                self.compiled, passed, total
            );
            check.evidence = source.trim().chars().take(100).collect();
        }
        grade.summary = format!(
            "本机样例通过 {}/{}；不代表覆盖所有输入。\n{}",
            passed, total, self.output
        );
        grade.coding = Some(self.clone());
        grade
    }
}
pub fn compiler(root: &Path, language: &str) -> PathBuf {
    if language == "C" {
        root.join("tcc/tcc.exe")
    } else {
        std::env::var_os("JAVA_HOME")
            .map(|p| PathBuf::from(p).join("bin/javac.exe"))
            .filter(|p| p.is_file())
            .unwrap_or_else(|| "javac".into())
    }
}
fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}
pub fn run(lab: &Lab, source: &str, root: &Path, cancel: &AtomicBool) -> Report {
    let started = Instant::now();
    let mut report = Report {
        total: lab.cases.len(),
        ..Default::default()
    };
    let result = (|| -> Result<(), String> {
        lab.validate()?;
        if source.len() > 65_536 {
            return Err("源码超过 64 KiB".into());
        }
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let compiler = compiler(root, &lab.language);
        if lab.language == "C" && !compiler.is_file() {
            return Err("请在练习设置中安装轻量 C 编译器".into());
        }
        let filename = if lab.language == "Java" {
            "Main.java"
        } else {
            "main.c"
        };
        std::fs::write(dir.path().join(filename), source).map_err(|e| e.to_string())?;
        let mut build = Command::new(&compiler);
        if lab.language == "Java" {
            build.args(["-encoding", "UTF-8", "Main.java"]);
        } else {
            build
                .arg("-B")
                .arg(root.join("tcc"))
                .args(["main.c", "-o", "answer.exe"]);
        }
        let (ok, out, err) =
            crate::runner::run(&mut build, dir.path(), "", Duration::from_secs(20), cancel)?;
        report.compiled = ok;
        report.output = format!("{out}{err}");
        if !ok {
            return Ok(());
        }
        for case in &lab.cases {
            let mut command = if lab.language == "Java" {
                let sibling = compiler.with_file_name("java.exe");
                let mut c = Command::new(if sibling.is_file() {
                    sibling
                } else {
                    "java".into()
                });
                c.args([
                    "-Xmx128m",
                    "-XX:ActiveProcessorCount=2",
                    "-Dfile.encoding=UTF-8",
                    "-cp",
                    ".",
                    "Main",
                ]);
                c
            } else {
                Command::new(dir.path().join("answer.exe"))
            };
            let result = crate::runner::run(
                &mut command,
                dir.path(),
                &case.input,
                Duration::from_secs(3),
                cancel,
            );
            let (passed, actual, error) = match result {
                Ok((ok, out, err)) => (
                    ok && normalized(&out) == normalized(&case.expected),
                    out,
                    err,
                ),
                Err(e) => {
                    report.output.push_str(&e);
                    report.cases.push(CaseReport {
                        input: case.input.clone(),
                        expected: case.expected.clone(),
                        actual: String::new(),
                        error: e,
                        passed: false,
                    });
                    break;
                }
            };
            report.cases.push(CaseReport {
                input: case.input.clone(),
                expected: case.expected.clone(),
                actual,
                error,
                passed,
            });
        }
        Ok(())
    })();
    if let Err(error) = result {
        report.output.push_str(&error);
    }
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    report
}
pub fn cards() -> Vec<crate::study::Card> {
    serde_json::from_str(include_str!("../assets/coding.json")).expect("validated coding bank")
}
