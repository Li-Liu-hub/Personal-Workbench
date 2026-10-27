use super::{Result, err};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

/// Windows sharing rules release this cross-process lease even after a crash.
/// Keep the file in place: unlinking it would permit a second lock inode.
pub struct Lease {
    _file: File,
}
impl Lease {
    pub fn acquire(directory: &Path) -> Result<Self> {
        Self::named(directory, "browser-operation.lock")
    }
    pub fn coordinator(directory: &Path) -> Result<Self> {
        Self::named(directory, "hiring-task.lock")
    }
    pub fn coordinator_busy(directory: &Path) -> bool {
        let path = directory.join("hiring-task.lock");
        if !path.exists() {
            return false;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        options.open(path).is_err()
    }
    pub fn account(directory: &Path, account_id: &str) -> Result<Self> {
        let id = super::accounts::id(account_id)?;
        if id == super::accounts::DEFAULT_ACCOUNT {
            Self::acquire(directory)
        } else {
            Self::acquire(&directory.join("accounts").join(id))
        }
    }
    fn named(directory: &Path, name: &str) -> Result<Self> {
        std::fs::create_dir_all(directory).map_err(err)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let path = directory.join(name);
        for attempt in 0..10 {
            match options.open(&path) {
                Ok(file) => return Ok(Self { _file: file }),
                Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) && attempt < 9 => {
                    std::thread::sleep(std::time::Duration::from_millis(25))
                }
                Err(_) => break,
            }
        }
        Err("另一个求职任务正在操作浏览器，请等待其完成或停止队列".into())
    }
}
