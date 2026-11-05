// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--apply-update")) {
    let result = std::env::args_os().nth(2)
      .ok_or_else(|| "缺少更新请求".to_string())
      .and_then(|path| personal_desk::updater::apply_request(std::path::Path::new(&path)));
    if let Err(error) = result { eprintln!("更新失败：{error}"); std::process::exit(1); }
    return;
  }
  personal_desk_lib::run();
}
