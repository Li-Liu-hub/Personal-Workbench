use std::sync::OnceLock;

#[derive(serde::Deserialize)]
pub(super) struct ReleaseNotes {
    pub version: String,
    pub title: String,
    pub changes: Vec<String>,
}

pub(super) fn releases() -> &'static [ReleaseNotes] {
    static RELEASES: OnceLock<Vec<ReleaseNotes>> = OnceLock::new();
    RELEASES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/release_history.json"))
            .expect("embedded release history must be valid")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_covers_current_release_and_has_unique_descending_versions() {
        let releases = releases();
        assert_eq!(releases[0].version, crate::updater::VERSION);
        let mut previous = None;
        for release in releases {
            let version = semver::Version::parse(&release.version).unwrap();
            if let Some(previous) = previous {
                assert!(
                    version < previous,
                    "duplicate or unordered version: {version}"
                );
            }
            previous = Some(version);
            assert!(!release.title.trim().is_empty());
            assert!(!release.changes.is_empty());
            assert!(
                release
                    .changes
                    .iter()
                    .all(|change| !change.trim().is_empty())
            );
        }
        assert_eq!(releases.last().unwrap().version, "0.1.0");
    }
}
