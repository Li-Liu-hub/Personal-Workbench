//! Manually saved company recruiting websites retained for the PersonalDesk data store.

use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Site {
    pub id: String,
    pub company: String,
    pub position: String,
    pub url: String,
    pub location: String,
    pub notes: String,
    pub revision: i64,
}

impl Site {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            ..Self::default()
        }
    }

    pub fn validate(&mut self) -> Result<(), String> {
        self.company = self.company.trim().into();
        if self.company.is_empty() {
            return Err("请填写公司名称".into());
        }
        if self.company.chars().count() > 200
            || self.position.chars().count() > 300
            || self.notes.chars().count() > 10000
        {
            return Err("公司、岗位或备注过长".into());
        }
        let mut url =
            url::Url::parse(self.url.trim()).map_err(|_| "请填写完整的 http / https 网址")?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("请填写完整的 http / https 网址".into());
        }
        url.set_fragment(None);
        self.url = url.into();
        Ok(())
    }
}
