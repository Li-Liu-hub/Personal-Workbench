use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct WritingImportBook {
    pub book: Book,
    pub pages: Vec<WritingImportPage>,
}

#[derive(Deserialize)]
pub struct WritingImportPage {
    pub source: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct WritingSummary {
    pub id: String,
    pub book_id: String,
    pub title: String,
    pub preview: String,
    pub tags: Vec<String>,
    pub updated_at: String,
}
/// 图书分组；包含父图书 ID、标题和标签。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    pub parent_id: Option<String>,
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
}
impl Book {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: None,
            title: "未命名图书".into(),
            tags: vec![],
        }
    }
}

impl Default for Book {
    fn default() -> Self {
        Self::new()
    }
}
/// 写作文档页面；包含所属图书、标题、正文、标签、修订号和更新时间。
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
