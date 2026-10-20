use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc::{self, Receiver},
    thread,
};
pub struct Terminal {
    pub parser: vt100::Parser<Responses>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    input: mpsc::SyncSender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
    size: (u16, u16),
}
#[derive(Default)]
pub struct Responses(Vec<u8>);
impl vt100::Callbacks for Responses {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        prefix: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        command: char,
    ) {
        if prefix.is_none() && command == 'n' {
            match params.first().and_then(|value| value.first()) {
                Some(6) => {
                    let (row, col) = screen.cursor_position();
                    self.0
                        .extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
                }
                Some(5) => self.0.extend_from_slice(b"\x1b[0n"),
                _ => {}
            }
        } else if prefix.is_none() && command == 'c' {
            self.0.extend_from_slice(b"\x1b[?1;0c");
        }
    }
}
impl Terminal {
    pub fn start(directory: &Path) -> Result<Self, String> {
        Self::spawn(directory, None)
    }
    #[cfg(debug_assertions)]
    pub fn start_command(directory: &Path, script: &str) -> Result<Self, String> {
        Self::spawn(directory, Some(script))
    }
    fn spawn(directory: &Path, script: Option<&str>) -> Result<Self, String> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let shell = if Path::new("C:/Program Files/PowerShell/7/pwsh.exe").exists() {
            "C:/Program Files/PowerShell/7/pwsh.exe"
        } else {
            "powershell.exe"
        };
        let mut command = CommandBuilder::new(shell);
        command.arg("-NoLogo");
        command.arg("-NoProfile");
        if let Some(script) = script {
            command.args(["-NonInteractive", "-Command", script]);
        }
        command.cwd(directory);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let (input, pending) = mpsc::sync_channel::<Vec<u8>>(32);
        thread::spawn(move || {
            while let Ok(bytes) = pending.recv() {
                if writer
                    .write_all(&bytes)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let (sender, receiver) = mpsc::sync_channel(64);
        thread::spawn(move || {
            let mut bytes = [0; 8192];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if sender.send(bytes[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self {
            parser: vt100::Parser::new_with_callbacks(30, 100, 10000, Responses::default()),
            master: pair.master,
            child,
            input,
            receiver,
            size: (30, 100),
        })
    }
    pub fn poll(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(8);
        for _ in 0..32 {
            let Ok(bytes) = self.receiver.try_recv() else {
                break;
            };
            self.parser.process(&bytes);
            let responses = std::mem::take(&mut self.parser.callbacks_mut().0);
            if !responses.is_empty() {
                let _ = self.input.try_send(responses);
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
        }
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.parser.screen_mut().set_scrollback(0);
        if bytes.len() > 1024 * 1024 {
            return Err("输入内容过长，请分段粘贴".into());
        }
        self.input
            .try_send(bytes.to_vec())
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "终端输入繁忙，请稍后重试".into(),
                mpsc::TrySendError::Disconnected(_) => "PowerShell 已退出，请重新启动终端".into(),
            })
    }
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let size = (rows.max(5), cols.max(20));
        if self.size != size
            && self
                .master
                .resize(PtySize {
                    rows: size.0,
                    cols: size.1,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .is_ok()
        {
            self.parser.screen_mut().set_size(size.0, size.1);
            self.size = size;
        }
    }
    pub fn active(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.receiver = mpsc::channel().1;
        self.input = mpsc::sync_channel(1).0;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_input_and_busy_output_do_not_block_the_ui_or_shutdown() {
        let (done, result) = mpsc::channel();
        thread::spawn(move || {
            let root = tempfile::tempdir().unwrap();
            let mut terminal = Terminal::start_command(
                root.path(),
                "[Console]::Write(('X'*2000000)); Start-Sleep -Seconds 20",
            )
            .unwrap();
            for _ in 0..15 {
                terminal.poll();
                thread::sleep(std::time::Duration::from_millis(20));
            }
            let start = std::time::Instant::now();
            terminal.write(&vec![b'a'; 512 * 1024]).unwrap();
            let duration = start.elapsed();
            drop(terminal);
            done.send(duration).unwrap();
        });
        let duration = result
            .recv_timeout(std::time::Duration::from_secs(8))
            .expect("terminal shutdown blocked on a full pipe");
        assert!(
            duration < std::time::Duration::from_millis(100),
            "{duration:?}"
        );
    }
    #[test]
    fn interactive_powershell_accepts_typed_commands_and_enter() {
        let root = tempfile::tempdir().unwrap();
        let mut terminal = Terminal::start(root.path()).unwrap();
        let ready = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < ready {
            terminal.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        terminal
            .write(b"[Console]::WriteLine(('PTY_VALUE_'+(40+2)))\r")
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);
        while std::time::Instant::now() < deadline {
            terminal.poll();
            if terminal.parser.screen().contents().contains("PTY_VALUE_42") {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!(
            "interactive output: {:?}",
            terminal.parser.screen().contents()
        );
    }
    #[test]
    fn conpty_powershell_produces_real_output_in_the_requested_directory() {
        let root = tempfile::tempdir().unwrap();
        let mut terminal = Terminal::start_command(
            root.path(),
            "Write-Output 'PERSONAL_DESK_PTY_OK'; (Get-Location).Path",
        )
        .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            terminal.poll();
            let output = terminal.parser.screen().contents();
            if output.contains("PERSONAL_DESK_PTY_OK")
                && output.contains(&root.path().to_string_lossy().to_string())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let output = terminal.parser.screen().contents();
        assert!(output.contains("PERSONAL_DESK_PTY_OK"), "{output:?}");
        assert!(
            output.contains(&root.path().to_string_lossy().to_string()),
            "{output:?}"
        );
    }
}
