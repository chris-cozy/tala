//! Versioned collection snapshots and validated restore. Staged files and a recovery
//! journal keep interrupted replacement from discarding the previous collection.

use crate::{
    content::valid_media_id,
    error::{AppError, Result},
    files::atomic_write,
    models::BackupInfo,
    store::{Store, id},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format_version: u32,
    schema_version: u32,
    created_at: i64,
    files: BTreeMap<String, String>,
}

fn digest_file(path: &Path) -> Result<String> {
    let mut reader = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
fn remove_dir_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Only run during process initialization, before any backup job can exist.
pub(crate) fn cleanup_abandoned_snapshots(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name
            .strip_prefix(".snapshot-")
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok())
            && entry.file_type().is_ok_and(|kind| kind.is_dir())
            && let Err(error) = fs::remove_dir_all(entry.path())
        {
            log::warn!("Could not clean an abandoned backup snapshot: {error}");
        }
    }
}

/// Owns an isolated SQLite snapshot and media links; dropping it removes temporary files.
/// Once prepared under the store lock, compression can safely run without that lock.
pub struct PreparedArchive {
    directory: PathBuf,
    created_at: i64,
    paths: Vec<(String, PathBuf)>,
}
impl Drop for PreparedArchive {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
impl PreparedArchive {
    pub fn write(&self, target: &Path) -> Result<()> {
        let temp = target.with_file_name(format!(".tala-{}.tmp", id()));
        let result = (|| -> Result<()> {
            let mut hashes = BTreeMap::new();
            for (name, path) in &self.paths {
                let hash = digest_file(path)?;
                if let Some(media) = name.strip_prefix("media/")
                    && !media.starts_with(&format!("{hash}."))
                {
                    return Err(AppError::invalid(
                        "An image has changed on disk. Run an integrity check before backing up.",
                    ));
                }
                hashes.insert(name.clone(), hash);
            }
            let manifest = Manifest {
                format_version: 1,
                schema_version: 1,
                created_at: self.created_at,
                files: hashes,
            };
            let mut zip = ZipWriter::new(File::create(&temp)?);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("manifest.json", options)?;
            zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
            for (name, path) in &self.paths {
                zip.start_file(name, options)?;
                std::io::copy(&mut File::open(path)?, &mut zip)?;
            }
            zip.finish()?.sync_all()?;
            fs::rename(&temp, target)?;
            if let Some(parent) = target.parent() {
                File::open(parent)?.sync_all()?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}
pub struct AutomaticBackup {
    snapshot: PreparedArchive,
    target: PathBuf,
    instance: String,
    day: String,
    counter: u64,
}
impl AutomaticBackup {
    pub fn write(&self) -> Result<()> {
        self.snapshot.write(&self.target)
    }
}

impl Store {
    pub fn prepare_archive(&self) -> Result<PreparedArchive> {
        let directory = self.root.join(format!(".snapshot-{}", id()));
        fs::create_dir_all(directory.join("media"))?;
        let mut snapshot = PreparedArchive {
            directory,
            created_at: self.now().now,
            paths: Vec::new(),
        };
        let database = snapshot.directory.join("tala.sqlite3");
        self.conn.backup("main", &database, None)?;
        snapshot.paths.push(("tala.sqlite3".into(), database));
        let media = self
            .conn
            .prepare("SELECT id FROM media ORDER BY id")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for media_id in media {
            if !valid_media_id(&media_id) {
                return Err(AppError::invalid(
                    "A media reference is invalid. Run an integrity check before backing up.",
                ));
            }
            let source = self.media_dir().join(&media_id);
            if !source.is_file() {
                return Err(AppError::invalid(
                    "A referenced image is missing. Run an integrity check before creating a complete backup.",
                ));
            }
            let destination = snapshot.directory.join("media").join(&media_id);
            // Media is immutable and content-addressed. Hard links protect the snapshot
            // from concurrent cleanup or restore without copying image bytes under the lock.
            if fs::hard_link(&source, &destination).is_err() {
                fs::copy(source, &destination)?;
            }
            snapshot
                .paths
                .push((format!("media/{media_id}"), destination));
        }
        Ok(snapshot)
    }
    pub fn write_archive(&self, target: &Path) -> Result<()> {
        self.prepare_archive()?.write(target)
    }
    pub fn prepare_automatic_backup(&mut self) -> Result<Option<AutomaticBackup>> {
        if !self.preferences()?.backup_enabled {
            return Ok(None);
        }
        let counter = self.metadata::<u64>("change_counter")?.unwrap_or(0);
        let day = self.now().day();
        if counter <= self.metadata::<u64>("last_auto_counter")?.unwrap_or(0)
            || self.metadata::<String>("last_auto_day")?.as_deref() == Some(&day)
        {
            return Ok(None);
        }
        let name = format!(
            "auto-{}-{}.tala",
            chrono::DateTime::from_timestamp(self.now().now, 0)
                .unwrap()
                .format("%Y%m%d-%H%M%S"),
            &id()[..8]
        );
        let snapshot = match self.prepare_archive() {
            Ok(value) => value,
            Err(error) => {
                self.backup_warning = Some(error.message.clone());
                return Err(error);
            }
        };
        Ok(Some(AutomaticBackup {
            snapshot,
            target: self.root.join("backups").join(name),
            instance: self.instance_id.clone(),
            day,
            counter,
        }))
    }
    pub fn finish_automatic_backup(
        &mut self,
        job: AutomaticBackup,
        result: Result<()>,
    ) -> Result<()> {
        // Restore changes the instance even when the data directory stays the same.
        if job.instance != self.instance_id {
            return result;
        }
        if let Err(error) = result {
            self.backup_warning = Some(error.message.clone());
            return Err(error);
        }
        self.set_metadata("last_auto_day", &job.day)?;
        self.set_metadata("last_auto_counter", &job.counter)?;
        let retention = self.preferences()?.backup_retention as usize;
        for backup in self
            .backups()?
            .into_iter()
            .filter(|b| b.automatic)
            .skip(retention)
        {
            fs::remove_file(self.root.join("backups").join(backup.name))?;
        }
        self.backup_warning = None;
        Ok(())
    }
    pub fn create_backup(&mut self, automatic: bool) -> Result<BackupInfo> {
        let name = format!(
            "{}-{}-{}.tala",
            if automatic { "auto" } else { "manual" },
            chrono::DateTime::from_timestamp(self.now().now, 0)
                .unwrap()
                .format("%Y%m%d-%H%M%S"),
            &id()[..8]
        );
        let target = self.root.join("backups").join(&name);
        self.write_archive(&target)?;
        if automatic {
            self.set_metadata("last_auto_day", &self.now().day())?;
            self.set_metadata(
                "last_auto_counter",
                &self.metadata::<u64>("change_counter")?.unwrap_or(0),
            )?;
            let retention = self.preferences()?.backup_retention as usize;
            for backup in self
                .backups()?
                .into_iter()
                .filter(|b| b.automatic)
                .skip(retention)
            {
                fs::remove_file(self.root.join("backups").join(backup.name))?;
            }
        }
        self.backup_warning = None;
        Ok(BackupInfo {
            name,
            created_at: self.now().now,
            bytes: fs::metadata(target)?.len(),
            automatic,
        })
    }
    pub fn maybe_backup(&mut self) -> Result<()> {
        if let Some(job) = self.prepare_automatic_backup()? {
            let result = job.write();
            self.finish_automatic_backup(job, result)?;
        }
        Ok(())
    }
    pub fn backups(&self) -> Result<Vec<BackupInfo>> {
        let mut backups = Vec::new();
        for entry in fs::read_dir(self.root.join("backups"))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".tala") {
                continue;
            }
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            let created_at = metadata
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            backups.push(BackupInfo {
                automatic: name.starts_with("auto-"),
                name,
                created_at,
                bytes: metadata.len(),
            });
        }
        backups.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.name.cmp(&a.name))
        });
        Ok(backups)
    }
    pub fn export_native(&self, token: &str) -> Result<()> {
        self.write_archive(&self.granted_path(token, "native")?)
    }
    pub fn restore_named(&mut self, name: &str) -> Result<()> {
        if name.contains('/')
            || name.contains('\\')
            || !self.backups()?.iter().any(|b| b.name == name)
        {
            return Err(AppError::invalid("Choose an existing Tala backup."));
        }
        self.restore_archive(&self.root.join("backups").join(name))
    }
    pub fn restore_token(&mut self, token: &str) -> Result<()> {
        self.restore_archive(&self.granted_path(token, "restore")?)
    }

    fn extract_archive(path: &Path, stage: &Path) -> Result<()> {
        if fs::metadata(path)?.len() > 64 * 1024 * 1024 * 1024 {
            return Err(AppError::invalid(
                "This backup exceeds the 64 GB archive limit.",
            ));
        }
        let mut zip = ZipArchive::new(File::open(path)?)?;
        let mut manifest_entry = zip.by_name("manifest.json")?;
        if manifest_entry.size() > 16 * 1024 * 1024 {
            return Err(AppError::invalid("The backup manifest is too large."));
        }
        let mut bytes = Vec::new();
        manifest_entry.read_to_end(&mut bytes)?;
        drop(manifest_entry);
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        if manifest.format_version != 1 || manifest.schema_version != 1 {
            return Err(AppError::invalid(
                "This backup requires a different version of Tala.",
            ));
        }
        if !manifest.files.contains_key("tala.sqlite3") || zip.len() != manifest.files.len() + 1 {
            return Err(AppError::invalid("The backup file list is incomplete."));
        }
        fs::create_dir_all(stage.join("media"))?;
        let mut seen = HashSet::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            if !seen.insert(name.clone()) {
                return Err(AppError::invalid(
                    "The archive contains duplicate file paths.",
                ));
            }
            if name == "manifest.json" {
                continue;
            }
            if entry.is_dir()
                || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
                || !(name == "tala.sqlite3"
                    || name.strip_prefix("media/").is_some_and(valid_media_id))
            {
                return Err(AppError::invalid(
                    "The archive contains an unsafe or unsupported path.",
                ));
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| AppError::invalid("Invalid archive size."))?;
            if total > 128 * 1024 * 1024 * 1024 || entry.size() > 8 * 1024 * 1024 * 1024 {
                return Err(AppError::invalid(
                    "The extracted backup exceeds safe size limits.",
                ));
            }
            let expected = manifest
                .files
                .get(&name)
                .ok_or_else(|| AppError::invalid("An archive file is not in the manifest."))?;
            let target = stage.join(&name);
            let mut out = File::create(&target)?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65536];
            let mut written = 0u64;
            loop {
                let n = entry.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                written += n as u64;
                if written > entry.size() {
                    return Err(AppError::invalid("An archive entry has an invalid size."));
                }
                hash.update(&buffer[..n]);
                out.write_all(&buffer[..n])?;
            }
            out.sync_all()?;
            if hex::encode(hash.finalize()) != *expected {
                return Err(AppError::invalid(
                    "A backup checksum did not match. The current collection has not been changed.",
                ));
            }
        }
        let conn = Connection::open_with_flags(
            stage.join("tala.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if check != "ok" || version != 1 {
            return Err(AppError::invalid(
                "The backup database is damaged or unsupported.",
            ));
        }
        let report = crate::integrity::check(&conn, &stage.join("media"))?;
        if !report.healthy {
            return Err(AppError::invalid(
                "The backup failed collection validation. The current collection has not been changed.",
            ));
        }
        let references = conn
            .prepare("SELECT id FROM media")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for reference in references {
            if !valid_media_id(&reference) || !stage.join("media").join(&reference).is_file() {
                return Err(AppError::invalid(
                    "The backup has missing or invalid media.",
                ));
            }
        }
        Ok(())
    }
    /// Recovery also works when SQLite cannot open the current database.
    /// The original directory is preserved intact, including any WAL and media files.
    pub fn recover_archive(root: &Path, path: &Path) -> Result<()> {
        let stage = root.join("restore-stage");
        let previous = root.join("restore-previous");
        let collection = root.join("collection");
        let journal = root.join("restore-journal.json");
        remove_dir_if_exists(&stage)?;
        if let Err(error) = Self::extract_archive(path, &stage) {
            let _ = remove_dir_if_exists(&stage);
            return Err(error);
        }
        let preserved = root.join("recovery-preserved").join(format!(
            "{}-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            id()
        ));
        fn copy_tree(source: &Path, target: &Path) -> Result<()> {
            fs::create_dir_all(target)?;
            for entry in fs::read_dir(source)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                let destination = target.join(entry.file_name());
                if kind.is_symlink() {
                    return Err(AppError::invalid(
                        "Recovery stopped because the existing collection contains a symbolic link. Preserve the files manually before continuing.",
                    ));
                }
                if kind.is_dir() {
                    copy_tree(&entry.path(), &destination)?;
                } else {
                    fs::copy(entry.path(), &destination)?;
                    File::open(destination)?.sync_all()?;
                }
            }
            File::open(target)?.sync_all()?;
            Ok(())
        }
        if collection.exists() {
            copy_tree(&collection, &preserved)?;
        }
        remove_dir_if_exists(&previous)?;
        atomic_write(
            &journal,
            b"{\"version\":1,\"operation\":\"recover-collection\"}",
        )?;
        let switched = (|| -> Result<()> {
            if collection.exists() {
                fs::rename(&collection, &previous)?;
            }
            fs::rename(&stage, &collection)?;
            let conn = Connection::open(collection.join("tala.sqlite3"))?;
            if !crate::integrity::check(&conn, &collection.join("media"))?.healthy {
                return Err(AppError::invalid("Recovered data failed validation."));
            }
            fs::remove_file(&journal)?;
            Ok(())
        })();
        if let Err(error) = switched {
            Self::recover_restore(root)?;
            return Err(error);
        }
        remove_dir_if_exists(&previous)?;
        Ok(())
    }
    pub fn recover_restore(root: &Path) -> Result<()> {
        let journal = root.join("restore-journal.json");
        if !journal.exists() {
            return Ok(());
        }
        let previous = root.join("restore-previous");
        let current = root.join("collection");
        if previous.exists() {
            remove_dir_if_exists(&current)?;
            fs::rename(previous, current)?;
        }
        remove_dir_if_exists(&root.join("restore-stage"))?;
        fs::remove_file(journal)?;
        Ok(())
    }
    pub fn restore_archive(&mut self, path: &Path) -> Result<()> {
        let stage = self.root.join("restore-stage");
        let previous = self.root.join("restore-previous");
        let collection = self.root.join("collection");
        let journal = self.root.join("restore-journal.json");
        remove_dir_if_exists(&stage)?;
        if let Err(error) = Self::extract_archive(path, &stage) {
            let _ = remove_dir_if_exists(&stage);
            return Err(error);
        }
        // Restoration is never allowed to replace the only copy of the current collection.
        self.create_backup(false)?;
        remove_dir_if_exists(&previous)?;
        self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        atomic_write(
            &journal,
            b"{\"version\":1,\"operation\":\"replace-collection\"}",
        )?;
        let old = std::mem::replace(&mut self.conn, Connection::open_in_memory()?);
        drop(old);
        let switched = (|| -> Result<()> {
            fs::rename(&collection, &previous)?;
            fs::rename(&stage, &collection)?;
            self.conn = Connection::open(collection.join("tala.sqlite3"))?;
            self.conn.execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )?;
            self.conn.busy_timeout(std::time::Duration::from_secs(5))?;
            let check = self.integrity()?;
            if !check.healthy {
                return Err(AppError::invalid(
                    "The restored collection failed integrity validation. Your previous collection will be recovered.",
                ));
            }
            Ok(())
        })();
        if let Err(error) = switched {
            let invalid = std::mem::replace(&mut self.conn, Connection::open_in_memory()?);
            drop(invalid);
            Self::recover_restore(&self.root)?;
            self.conn = Connection::open(collection.join("tala.sqlite3"))?;
            self.conn.execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )?;
            return Err(error);
        }
        fs::remove_file(journal)?;
        remove_dir_if_exists(&previous)?;
        self.instance_id = id();
        self.grants.clear();
        self.backup_warning = None;
        Ok(())
    }
}
