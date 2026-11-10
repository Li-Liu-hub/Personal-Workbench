//! Tauri 命令实现，按业务领域拆分。命令名与参数键名是前端契约，不要改名。

pub mod contacts;
pub mod credentials;
pub mod essays;
pub mod export;
pub mod inventory;
pub mod mongo;
pub mod settings;
pub mod shortcuts;
pub mod theme;
pub mod tools;
pub mod websites;
pub mod writing;
