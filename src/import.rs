use crate::{
    db::Database,
    error::{DeskError, Result},
    model::{Credential, Website},
    websites,
};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
pub struct Bundle {
    pub accounts: Vec<Credential>,
    #[serde(default)]
    pub websites: Vec<Website>,
}

pub fn records(data_dir: &Path, mut bundle: Bundle) -> Result<(usize, usize)> {
    for account in &mut bundle.accounts {
        if account.title.trim().is_empty() {
            return Err(DeskError::InvalidPath("导入账号需要名称".into()));
        }
        if account.id.is_empty() {
            account.id = uuid::Uuid::new_v4().to_string();
        }
    }
    let websites = bundle
        .websites
        .into_iter()
        .map(|mut item| {
            if item.id.is_empty() {
                item.id = uuid::Uuid::new_v4().to_string();
            }
            websites::prepare(item)
        })
        .collect::<Result<Vec<_>>>()?;
    Database::open(data_dir)?.import_records(&bundle.accounts, &websites)
}

pub fn file(data_dir: &Path, path: &Path) -> Result<(usize, usize)> {
    let bytes = std::fs::read(path)?;
    records(data_dir, serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_bundle_is_rejected_before_creating_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let mut account = Credential::new();
        account.title = " \n".into();
        assert!(
            records(
                &data,
                Bundle {
                    accounts: vec![account],
                    websites: vec![]
                }
            )
            .is_err()
        );
        assert!(!data.exists());
        let input = dir.path().join("input.json");
        std::fs::write(&input, b"invalid JSON").unwrap();
        assert!(file(&data, &input).is_err());
        assert!(!data.exists());
    }

    #[test]
    fn generated_ids_are_persisted_and_replay_skips_normalized_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let input = || Bundle {
            accounts: vec![Credential {
                title: "example".into(),
                username: "user".into(),
                ..Default::default()
            }],
            websites: vec![Website {
                url: "example.test".into(),
                ..Default::default()
            }],
        };
        assert_eq!(records(dir.path(), input()).unwrap(), (1, 1));
        assert_eq!(records(dir.path(), input()).unwrap(), (0, 0));
        let db = Database::open(dir.path()).unwrap();
        assert!(uuid::Uuid::parse_str(&db.credentials().unwrap()[0].id).is_ok());
        assert!(uuid::Uuid::parse_str(&db.websites().unwrap()[0].id).is_ok());
    }
}
