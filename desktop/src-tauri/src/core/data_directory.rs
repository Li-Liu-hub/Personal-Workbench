use std::path::{Path, PathBuf};

fn pinned_directory(executable: &Path) -> Option<PathBuf> {
    let pin = executable.parent()?.join("personal-desk-data-path.txt");
    let value = std::fs::read_to_string(pin).ok()?;
    let path = PathBuf::from(value.trim().trim_start_matches('\u{feff}'));
    path.is_absolute().then_some(path)
}

/// Resolve the same data location used by the existing desktop application.
pub fn resolve() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    pinned_directory(&executable)
        .or_else(crate::install::remembered_data_directory)
        .or_else(|| {
            directories::BaseDirs::new().map(|base| base.data_local_dir().join("PersonalDesk"))
        })
        .ok_or_else(|| "无法确定个人办公台数据目录".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_absolute_pinned_directory_and_ignores_relative_values() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("app.exe");
        let data = root.path().join("existing data");
        std::fs::write(
            root.path().join("personal-desk-data-path.txt"),
            format!("\u{feff}{}\n", data.display()),
        )
        .unwrap();
        assert_eq!(pinned_directory(&exe), Some(data));
        std::fs::write(root.path().join("personal-desk-data-path.txt"), "relative").unwrap();
        assert_eq!(pinned_directory(&exe), None);
    }
}
