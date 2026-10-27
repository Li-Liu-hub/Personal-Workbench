//! Bounded local child processes. This is resource control, not a security sandbox.
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

struct ChildProcess {
    child: Child,
    #[cfg(windows)]
    job: Option<crate::local_ai::ProcessJob>,
}
impl Drop for ChildProcess {
    fn drop(&mut self) {
        // Every return path, including polling/read errors, reaps the child.
        let _ = self.child.kill();
        let _ = self.child.wait();
        #[cfg(windows)]
        {
            // Kill remaining descendants before opening their output files.
            self.job = None;
        }
    }
}

pub fn run(
    command: &mut Command,
    directory: &Path,
    input: &str,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<(bool, String, String), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("已停止运行".into());
    }
    if timeout.is_zero() {
        return Err("运行超时，已停止".into());
    }
    fs::write(directory.join("stdin.txt"), input).map_err(|e| e.to_string())?;
    let out = fs::File::create(directory.join("stdout.txt")).map_err(|e| e.to_string())?;
    let err = fs::File::create(directory.join("stderr.txt")).map_err(|e| e.to_string())?;
    command
        .current_dir(directory)
        .stdin(fs::File::open(directory.join("stdin.txt")).map_err(|e| e.to_string())?)
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = command
        .spawn()
        .map_err(|e| format!("无法启动运行程序：{e}"))?;
    let mut process = ChildProcess {
        child,
        #[cfg(windows)]
        job: None,
    };
    #[cfg(windows)]
    {
        process.job = Some(
            crate::local_ai::ProcessJob::attach_limited(&process.child, 2 * 1024 * 1024 * 1024)
                .map_err(|e| e.to_string())?,
        );
    }
    let started = Instant::now();
    let success = loop {
        let error = if cancel.load(Ordering::Relaxed) {
            Some("已停止运行")
        } else if started.elapsed() > timeout {
            Some("运行超时，已停止")
        } else if ["stdout.txt", "stderr.txt"]
            .iter()
            .any(|p| fs::metadata(directory.join(p)).is_ok_and(|m| m.len() > 65_536))
        {
            Some("输出超过 64 KiB，已停止")
        } else {
            None
        };
        if let Some(error) = error {
            return Err(error.into());
        }
        if let Some(status) = process.child.try_wait().map_err(|e| e.to_string())? {
            break status.success();
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    drop(process);
    let read = |name| -> Result<String, String> {
        if fs::metadata(directory.join(name)).is_ok_and(|m| m.len() > 65_536) {
            return Err("输出超过 64 KiB，已停止".into());
        }
        let mut b = Vec::new();
        fs::File::open(directory.join(name))
            .map_err(|e| e.to_string())?
            .take(65_536)
            .read_to_end(&mut b)
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    };
    Ok((success, read("stdout.txt")?, read("stderr.txt")?))
}

#[cfg(test)]
mod tests;
