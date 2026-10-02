//! Native file grants, atomic exports, and immutable content-addressed media.
//! The frontend cannot choose arbitrary filesystem paths through collection commands.

use crate::{
    content::valid_media_id,
    error::{AppError, Result},
    models::*,
    store::{Grant, Store, id},
};
use rusqlite::params;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::invalid("Choose a valid file location."))?;
    let temp = parent.join(format!(".tala-{}.tmp", id()));
    let result = (|| -> Result<()> {
        let mut f = File::create(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&temp, path)?;
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
impl Store {
    pub fn grant(&mut self, path: PathBuf, purpose: String) -> FileSelection {
        let token = id();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.grants.insert(token.clone(), Grant { path, purpose });
        FileSelection { token, name }
    }
    pub fn granted_path(&self, token: &str, purpose: &str) -> Result<PathBuf> {
        self.grants
            .get(token)
            .filter(|g| g.purpose == purpose)
            .map(|g| g.path.clone())
            .ok_or_else(|| AppError::invalid("Select this file again using Tala’s file chooser."))
    }
    pub fn attach_image_file(&mut self, token: &str) -> Result<String> {
        let path = self.granted_path(token, "image")?;
        if fs::metadata(&path)?.len() > 20 * 1024 * 1024 {
            return Err(AppError::invalid("Images must be smaller than 20 MB."));
        }
        self.attach_image(&fs::read(path)?)
    }
    pub fn attach_image(&mut self, bytes: &[u8]) -> Result<String> {
        self.attach_media(bytes, false)
    }
    pub fn attach_audio_file(&mut self, token: &str) -> Result<String> {
        let path = self.granted_path(token, "audio")?;
        if fs::metadata(&path)?.len() > 20 * 1024 * 1024 {
            return Err(AppError::invalid(
                "Audio files must be no larger than 20 MB.",
            ));
        }
        self.attach_audio(&fs::read(path)?)
    }
    pub fn attach_audio(&mut self, bytes: &[u8]) -> Result<String> {
        self.attach_media(bytes, true)
    }
    pub(crate) fn attach_media(&mut self, bytes: &[u8], audio: bool) -> Result<String> {
        let info = crate::media::validate(bytes, audio)?;
        self.install_media(bytes, info.extension, info.mime)
    }
    pub(crate) fn install_media(
        &mut self,
        bytes: &[u8],
        extension: &str,
        mime: &str,
    ) -> Result<String> {
        let media_id = format!("{}.{}", hex::encode(Sha256::digest(bytes)), extension);
        let target = self.media_dir().join(&media_id);
        if target.exists() {
            if hex::encode(Sha256::digest(fs::read(&target)?)) != hex::encode(Sha256::digest(bytes))
            {
                return Err(AppError::invalid(
                    "An existing media file has changed on disk. Run an integrity check or restore a backup.",
                ));
            }
        } else {
            atomic_write(&target, bytes)?;
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO media(id,mime,bytes,created_at) VALUES (?1,?2,?3,?4)",
            params![media_id, mime, bytes.len() as i64, self.now().now],
        )?;
        self.dirty()?;
        Ok(media_id)
    }
    pub fn integrity(&self) -> Result<IntegrityReport> {
        let mut report = crate::integrity::check(&self.conn, &self.media_dir())?;
        match self.preferences() {
            Ok(preferences)
                if crate::scheduler::validate_settings(&preferences.defaults).is_ok()
                    && [90, 100, 110, 125].contains(&preferences.scale)
                    && (1..=100).contains(&preferences.backup_retention) => {}
            _ => report
                .issues
                .push("Application preferences are invalid.".into()),
        }
        if self.metadata::<SessionRecord>("session").is_err() || self.undo_record().is_err() {
            report
                .issues
                .push("Session or undo metadata is invalid.".into());
        }
        report.healthy = report.issues.is_empty() && report.missing_media.is_empty();
        Ok(report)
    }
    pub fn cleanup_media(&mut self) -> Result<u32> {
        let report = self.integrity()?;
        if !report.healthy {
            return Err(AppError::invalid(
                "Repair the reported collection issues before cleaning up media.",
            ));
        }
        let mut count = 0;
        for media in report.unused_media {
            if !valid_media_id(&media) {
                continue;
            }
            let path = self.media_dir().join(&media);
            if path.exists() {
                fs::remove_file(path)?;
            }
            self.conn
                .execute("DELETE FROM media WHERE id=?1", [media])?;
            count += 1;
        }
        if count > 0 {
            self.dirty()?;
        }
        Ok(count)
    }
    pub fn export_diagnostics(&self, token: &str) -> Result<()> {
        let target = self.granted_path(token, "diagnostics")?;
        let report = self.integrity()?;
        let text = serde_json::to_vec_pretty(
            &serde_json::json!({"application":"Tala","version":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"schemaVersion":crate::hierarchy::SCHEMA_VERSION,"integrity":report,"backupWarning":self.backup_warning}),
        )?;
        atomic_write(&target, &text)
    }
}
