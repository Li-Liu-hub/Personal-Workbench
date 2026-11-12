use super::*;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Cache {
    revision: u64,
    external_version: Option<i64>,
    checked: Option<Instant>,
    scopes: BTreeMap<String, ScopeData>,
}
struct ScopeData {
    names: Vec<String>,
    records: BTreeMap<String, Vec<String>>,
}
impl Database {
    fn refresh_tag_cache(&self, scope: TagScope) -> Result<()> {
        let mut cache = self.tag_cache.borrow_mut();
        let revision = self.desk.total_changes();
        if cache.revision != revision {
            cache.scopes.clear();
            cache.revision = revision;
        }
        // Other processes may import data; check once per second, not per card.
        if cache
            .checked
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
        {
            let version: i64 = self
                .desk
                .query_row("PRAGMA data_version", [], |r| r.get(0))?;
            if cache.external_version != Some(version) {
                cache.scopes.clear();
                cache.external_version = Some(version);
            }
            cache.checked = Some(Instant::now());
        }
        if !cache.scopes.contains_key(scope.key()) {
            let data = ScopeData {
                names: self.tags(scope)?,
                records: self
                    .tagged_records(scope)?
                    .into_iter()
                    .map(|r| (r.id, r.tags))
                    .collect(),
            };
            cache.scopes.insert(scope.key().into(), data);
        }
        Ok(())
    }
    pub fn cached_tag_names(&self, scope: TagScope) -> Result<Vec<String>> {
        self.refresh_tag_cache(scope)?;
        Ok(self.tag_cache.borrow().scopes[scope.key()].names.clone())
    }
    pub fn cached_record_tags(&self, scope: TagScope, id: &str) -> Result<Vec<String>> {
        self.refresh_tag_cache(scope)?;
        Ok(self.tag_cache.borrow().scopes[scope.key()]
            .records
            .get(id)
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tag_cache_refreshes_after_save_rename_delete_and_external_change() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let mut item = crate::model::Website::new();
        item.title = "synthetic".into();
        item.url = "https://example.test".into();
        db.save_website(&item).unwrap();
        let scope = TagScope::Websites;
        assert!(db.cached_record_tags(scope, &item.id).unwrap().is_empty());
        db.set_record_tags(scope, &item.id, &["one".into()])
            .unwrap();
        assert_eq!(db.cached_record_tags(scope, &item.id).unwrap(), vec!["one"]);
        db.rename_tag(scope, "one", "two").unwrap();
        assert_eq!(db.cached_record_tags(scope, &item.id).unwrap(), vec!["two"]);
        db.delete_tag(scope, "two").unwrap();
        assert!(db.cached_record_tags(scope, &item.id).unwrap().is_empty());
        let mut other = Database::open(dir.path()).unwrap();
        other
            .set_record_tags(scope, &item.id, &["external".into()])
            .unwrap();
        db.tag_cache.borrow_mut().checked = None;
        assert_eq!(
            db.cached_record_tags(scope, &item.id).unwrap(),
            vec!["external"]
        );
        assert_eq!(db.cached_tag_names(scope).unwrap(), vec!["external"]);
    }
}
