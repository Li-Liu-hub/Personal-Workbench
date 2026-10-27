use super::*;
use std::{process::Command, sync::atomic::AtomicBool, time::Duration};

pub struct Rendered {
    pub pdf: Vec<u8>,
    pub pages: Vec<Vec<u8>>,
    pub revision: Vec<u8>,
}
pub struct TypstRenderer {
    pub executable: PathBuf,
}
impl TypstRenderer {
    pub fn discover() -> Self {
        let mut candidates = vec![];
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            candidates.push(parent.join("tools/typst/typst.exe"));
        }
        candidates.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(".artifacts/resume/typst/typst-x86_64-pc-windows-msvc/typst.exe"),
        );
        Self {
            executable: candidates
                .into_iter()
                .find(|p| p.is_file())
                .unwrap_or_else(|| PathBuf::from("typst")),
        }
    }
    #[cfg(test)]
    pub fn render(
        &self,
        document: &Resume,
        store: &Store,
    ) -> std::result::Result<Rendered, String> {
        self.render_at_ppi(document, store, 300)
    }
    pub fn render_at_ppi(
        &self,
        document: &Resume,
        store: &Store,
        ppi: u32,
    ) -> std::result::Result<Rendered, String> {
        document.validate().map_err(|e| e.to_string())?;
        let work = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = work.path();
        fs::write(
            root.join("resume.json"),
            serde_json::to_vec_pretty(document).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::write(root.join("resume.typ"), TEMPLATE).map_err(|e| e.to_string())?;
        let font_names = [
            "ResumeNotoSansSC-Regular.ttf",
            "ResumeNotoSansSC-SemiBold.ttf",
        ];
        let original_fonts = font_names
            .iter()
            .all(|name| store.directory.join("fonts").join(name).is_file());
        if original_fonts {
            fs::create_dir(root.join("fonts")).map_err(|e| e.to_string())?;
            for name in font_names {
                fs::copy(
                    store.directory.join("fonts").join(name),
                    root.join("fonts").join(name),
                )
                .map_err(|e| format!("原版字体读取失败：{e}"))?;
            }
        }
        if !document.photo.is_empty() && document.style.show_photo {
            fs::copy(
                store.directory.join(&document.photo),
                root.join(&document.photo),
            )
            .map_err(|e| format!("照片读取失败：{e}"))?;
        }
        for output in ["resume.pdf", "page-{0p}.png"] {
            let mut command = Command::new(&self.executable);
            command.args(["compile", "--root", ".", "--ignore-system-fonts"]);
            if original_fonts {
                command.args(["--font-path", "fonts", "--input", "resume-font=noto"]);
            }
            // Windows system fonts are explicitly available; no package download is required.
            if cfg!(windows) {
                command.args(["--font-path", "C:/Windows/Fonts"]);
            }
            command.args(["resume.typ", output]);
            if output.ends_with("png") {
                command.args(["--ppi", &ppi.clamp(300, 600).to_string()]);
            }
            let (ok, _, error) = crate::runner::run(
                &mut command,
                root,
                "",
                Duration::from_secs(45),
                &AtomicBool::new(false),
            )
            .map_err(|e| format!("Typst 排版失败：{e}。可在此页选择 typst.exe。"))?;
            if !ok {
                return Err(format!("Typst 排版失败：{error}"));
            }
        }
        let mut paths = fs::read_dir(root)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("page-"))
                    && p.extension().is_some_and(|e| e == "png")
            })
            .collect::<Vec<_>>();
        paths.sort();
        if paths.is_empty() || paths.len() > 30 {
            return Err("简历页数为空或超过 30 页".into());
        }
        let pages = paths
            .iter()
            .map(fs::read)
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let pdf = fs::read(root.join("resume.pdf")).map_err(|e| e.to_string())?;
        Ok(Rendered {
            pdf,
            pages,
            revision: document.revision().map_err(|e| e.to_string())?,
        })
    }
}
