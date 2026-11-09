use rusqlite::{OptionalExtension, params};

use crate::error::Result;

use super::Database;

impl Database {
    pub fn blog_post_id(&self, page_id: &str) -> Result<Option<String>> {
        Ok(self
            .desk
            .query_row(
                "SELECT remote_post_id FROM blog_post_links WHERE local_page_id=?1",
                [page_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn link_blog_post(&mut self, page_id: &str, remote_post_id: &str) -> Result<()> {
        self.desk.execute(
            "INSERT INTO blog_post_links(local_page_id,remote_post_id) VALUES(?1,?2) \
             ON CONFLICT(local_page_id) DO UPDATE SET remote_post_id=excluded.remote_post_id",
            params![page_id, remote_post_id],
        )?;
        Ok(())
    }
}
