use super::*;

impl PersonalDeskApp {
    #[cfg(test)]
    pub(super) fn group_document_paths(&mut self, paths: &[PathBuf], book: Option<&str>) -> bool {
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.group_documents(paths, book))
        {
            Ok(_) => {
                self.notice = "已组成图书".into();
                true
            }
            Err(error) => {
                self.notice = error.to_string();
                false
            }
        }
    }
}
