fn main() {
    println!("cargo:rerun-if-changed=assets/personaldesk.ico");
    println!("cargo:rerun-if-env-changed=WINDRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set("FileDescription", "个人办公台")
        .set("ProductName", "个人办公台")
        .set("OriginalFilename", "PersonalDesk.exe");
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
