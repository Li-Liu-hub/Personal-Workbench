//! 编译脚本：把 assets/icon.ico 嵌入 exe（资源管理器/任务栏显示的头像）
//!
//! 只在 Windows 上生效；图标或 rc.exe 缺失时打印警告并跳过，不影响编译。
//!
//! 为什么要自己找 rc.exe：
//!   winresource 默认靠 `reg.exe` 查 Windows SDK 路径，
//!   而本机安全策略把 reg.exe 列进了程序黑名单 → 查不到路径 → 报
//!   「系统找不到指定的路径 (os error 3)」。
//!   所以这里直接扫 Windows Kits 目录，找到 rc.exe 后用 `set_toolkit_path`
//!   告诉 winresource，绕开注册表查询。

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        println!("cargo:rerun-if-changed=build.rs");

        let ico = std::path::Path::new("assets/icon.ico");
        if !ico.exists() {
            println!("cargo:warning=assets/icon.ico 不存在，exe 将不带图标");
            return;
        }

        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Excel 批量合并工具");
        res.set(
            "FileDescription",
            "选择文件夹，把其中所有 Excel 合并成一个 Excel",
        );

        // 手工定位 rc.exe，绕开 winresource 的 reg.exe 查询
        if let Some(rc_dir) = find_rc_dir() {
            res.set_toolkit_path(&rc_dir);
        } else {
            println!("cargo:warning=没找到 rc.exe，跳过嵌入 exe 图标");
            return;
        }

        if let Err(e) = res.compile() {
            println!("cargo:warning=嵌入 exe 图标失败（忽略）：{}", e);
        }
    }
}

/// 返回包含 rc.exe 的目录（优先最新的 SDK 版本、x64 架构）
#[cfg(windows)]
fn find_rc_dir() -> Option<String> {
    let bins = std::path::Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if !bins.is_dir() {
        return None;
    }

    // 收集形如 10.0.xxxxx.0 的版本目录，按版本号从新到旧
    let mut versions: Vec<(Vec<u32>, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(bins).ok()?.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let nums: Vec<u32> = name
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();
        if nums.len() >= 3 {
            versions.push((nums, p));
        }
    }
    versions.sort_by(|a, b| b.0.cmp(&a.0));

    for (_, ver_dir) in versions {
        for arch in ["x64", "x86"] {
            let d = ver_dir.join(arch);
            if d.join("rc.exe").exists() {
                return Some(d.to_string_lossy().to_string());
            }
        }
    }
    None
}
