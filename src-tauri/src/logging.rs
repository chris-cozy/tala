//! Best-effort local diagnostics, rotated at startup. Do not log note bodies,
//! imported rows, clipboard bytes, or full command payloads here or at call sites.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    sync::Mutex,
};
pub struct LocalLog {
    file: Mutex<File>,
}
impl log::Log for LocalLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata())
            && let Ok(mut file) = self.file.lock()
        {
            let _ = writeln!(
                file,
                "{} {} {}",
                chrono::Utc::now().to_rfc3339(),
                r.level(),
                r.args()
            );
        }
    }
    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}
pub fn initialize(root: &Path) {
    let folder = root.join("logs");
    if fs::create_dir_all(&folder).is_err() {
        return;
    }
    let path = folder.join("tala.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
        let _ = fs::rename(&path, folder.join("previous.log"));
    }
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(path)
        && log::set_boxed_logger(Box::new(LocalLog {
            file: Mutex::new(file),
        }))
        .is_ok()
    {
        log::set_max_level(log::LevelFilter::Info);
    }
}
