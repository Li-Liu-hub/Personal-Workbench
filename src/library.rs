use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
}
impl Book {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: "未命名图书".into(),
            tags: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub book_id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub revision: i64,
    pub updated_at: String,
}
impl Page {
    pub fn new(book_id: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            book_id,
            title: "未命名页面".into(),
            body: String::new(),
            tags: vec![],
            revision: 0,
            updated_at: String::new(),
        }
    }
}
