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
    writer: Box<dyn Write + Send>,
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
        if let Some(script) = script {
            command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        }
        command.cwd(directory);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
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
            writer,
            receiver,
            size: (30, 100),
        })
    }
    pub fn poll(&mut self) {
        while let Ok(bytes) = self.receiver.try_recv() {
            self.parser.process(&bytes);
            let responses = std::mem::take(&mut self.parser.callbacks_mut().0);
            if !responses.is_empty() {
                let _ = self
                    .writer
                    .write_all(&responses)
                    .and_then(|()| self.writer.flush());
            }
        }
    }
    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.parser.screen_mut().set_scrollback(0);
        self.writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
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
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
