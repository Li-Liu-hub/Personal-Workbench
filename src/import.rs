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
