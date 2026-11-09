fn windows_sdk_toolkit_path() -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("WindowsSdkVerBinPath") {
        let path = std::path::PathBuf::from(path);
        candidates.push(path.join("x64"));
        candidates.push(path);
    }
    if let (Some(root), Some(version)) = (
        std::env::var_os("WindowsSdkDir"),
        std::env::var_os("WindowsSDKVersion"),
    ) {
        let root = std::path::PathBuf::from(root);
        let version = version
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_owned();
        candidates.push(root.join("bin").join(&version).join("x64"));
        candidates.push(root);
    }
    if let Some(output) = std::process::Command::new("where.exe")
        .arg("rc.exe")
        .output()
        .ok()
        .filter(|output| output.status.success())
    {
        candidates.extend(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| std::path::PathBuf::from(line.trim()))
                .filter_map(|path| path.parent().map(std::path::Path::to_path_buf)),
        );
    }
    for bin in [
        std::path::PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin"),
        std::path::PathBuf::from(r"C:\Program Files\Windows Kits\10\bin"),
    ] {
        if let Ok(entries) = std::fs::read_dir(bin) {
            let mut versions = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect::<Vec<_>>();
            versions.sort();
            versions.reverse();
            candidates.extend(versions.iter().map(|path| path.join("x64")));
            candidates.extend(versions);
        }
    }
    candidates.push(std::path::PathBuf::from(
        r"C:\Program Files (x86)\Windows Kits\10",
    ));
    candidates.push(std::path::PathBuf::from(
        r"C:\Program Files\Windows Kits\10",
    ));
    candidates
        .into_iter()
        .find(|path| path.join("rc.exe").is_file())
}

fn main() {
    println!("cargo:rerun-if-changed=assets/personaldesk.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");
    println!("cargo:rerun-if-env-changed=WindowsSdkVerBinPath");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set("FileDescription", "个人办公台")
        .set("ProductName", "个人办公台")
        .set("OriginalFilename", "PersonalDesk.exe");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let sdk_bin = windows_sdk_toolkit_path();
        if let Some(sdk_bin) = sdk_bin {
            resource.set_toolkit_path(&sdk_bin.to_string_lossy());
        }
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        // MinGW's preprocessor cannot reliably handle Chinese absolute paths.
        // Keep its input paths relative to the build output directory.
        let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        std::fs::copy("assets/personaldesk.ico", output.join("personaldesk.ico"))
            .expect("failed to stage the PersonalDesk icon");
        resource
            .set_icon("personaldesk.ico")
            .write_resource_file(output.join("personaldesk.rc"))
            .expect("failed to write Windows resources");
        let architecture = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
        let target = match architecture.as_str() {
            "x86_64" => "pe-x86-64",
            "x86" => "pe-i386",
            _ => panic!("unsupported MinGW resource architecture: {architecture}"),
        };
        let status = std::process::Command::new(
            std::env::var_os("WINDRES").unwrap_or_else(|| "windres".into()),
        )
        .current_dir(&output)
        .args([
            "--input",
            "personaldesk.rc",
            "--output",
            "personaldesk.o",
            "--output-format",
            "coff",
            "--target",
            target,
        ])
        .status()
        .expect("failed to run windres; install MinGW or set WINDRES");
        assert!(status.success(), "failed to compile Windows resources");
        println!(
            "cargo:rustc-link-arg={}",
            output.join("personaldesk.o").display()
        );
    } else {
        resource
            .set_icon("assets/personaldesk.ico")
            .compile()
            .expect("failed to embed the PersonalDesk Windows icon");
    }
}
