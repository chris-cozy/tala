//! Checks source data and derived indexes before restore or media cleanup.
//! Repair only rebuilds derived fields; it never guesses missing note content or history.

use crate::{
    content::{normalized, plain_text, tag_name, valid_media_id, validate_document},
    error::{AppError, Result},
    models::*,
    scheduler::{valid_schedule, validate_settings},
    store::Store,
};
use rusqlite::{Connection, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

fn schema(conn: &Connection) -> Result<BTreeMap<String, String>> {
    Ok(conn
        .prepare("SELECT name,sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |row| Ok((row.get(0)?, row.get::<_, String>(1)?.split_whitespace().collect::<Vec<_>>().join(" "))))?
        .collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn validate_schema(conn: &Connection) -> Result<()> {
    let reference = Connection::open_in_memory()?;
    reference.execute_batch(include_str!("../migrations/001_initial.sql"))?;
    if schema(conn)? != schema(&reference)? {
        return Err(AppError::invalid(
            "This collection's database structure does not match Tala's supported schema. Restore a known-good backup; the current files have not been replaced.",
        ));
    }
    Ok(())
}

pub(crate) fn check(conn: &Connection, media_dir: &Path) -> Result<IntegrityReport> {
    validate_schema(conn)?;
    let mut issues = Vec::new();
    for check in conn
        .prepare("PRAGMA integrity_check")?
        .query_map([], |r| r.get::<_, String>(0))?
    {
        let check = check?;
        if check != "ok" {
            issues.push(check);
        }
    }
    for row in conn
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |r| r.get::<_, String>(0))?
    {
        issues.push(format!("A relationship is missing in {}.", row?));
    }
    for row in conn
        .prepare("SELECT id,schedule,phase,due FROM cards")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
    {
        let (id, raw, phase, due) = row?;
        if !serde_json::from_str::<Schedule>(&raw).is_ok_and(|schedule| {
            valid_schedule(&schedule) && schedule.phase.as_str() == phase && schedule.due == due
        }) {
            issues.push(format!("Invalid scheduling state for card {id}."));
        }
    }
    let orphan_notes: u32 = conn.query_row(
        "SELECT count(*) FROM notes n WHERE NOT EXISTS(SELECT 1 FROM cards c WHERE c.note_id=n.id)",
        [],
        |r| r.get(0),
    )?;
    if orphan_notes > 0 {
        issues.push(format!("{orphan_notes} notes have no generated card."));
    }
    let inconsistent: u32 = conn.query_row(
        "SELECT count(*) FROM cards c JOIN notes n ON n.id=c.note_id JOIN decks d ON \
            d.id=c.deck_id WHERE (c.deleted_at IS NULL)!=(n.deleted_at IS NULL) OR (c.deleted_at IS \
            NULL AND d.deleted_at IS NOT NULL)",
        [],
        |r| r.get(0),
    )?;
    if inconsistent > 0 {
        issues.push(format!(
            "{inconsistent} cards have inconsistent deletion state."
        ));
    }
    let mut referenced_media =
        conn.prepare("SELECT media_id FROM note_media WHERE note_id=?1 ORDER BY media_id")?;
    for row in conn
        .prepare("SELECT id,front,back,front_text,back_text,front_key,content_version FROM notes")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, u32>(6)?,
            ))
        })?
    {
        let (id, front, back, front_text, back_text, key, version) = row?;
        let validated = (|| -> Result<_> {
            let front: Value = serde_json::from_str(&front)?;
            let back: Value = serde_json::from_str(&back)?;
            let (safe_front, mut media) = validate_document(&front)?;
            let (safe_back, back_media) = validate_document(&back)?;
            if safe_front != front || safe_back != back || version != 1 {
                return Err(AppError::invalid("Unsupported or non-canonical content."));
            }
            media.extend(back_media);
            media.sort();
            media.dedup();
            Ok((plain_text(&front), plain_text(&back), media))
        })();
        match validated {
            Err(_) => issues.push(format!("Unsupported or invalid content for note {id}.")),
            Ok((front, back, media)) => {
                if front != front_text || back != back_text || normalized(&front) != key {
                    issues.push(format!("Search text needs rebuilding for note {id}."));
                }
                let registered = referenced_media
                    .query_map([&id], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                if media != registered {
                    issues.push(format!("Media references need rebuilding for note {id}."));
                }
            }
        }
    }
    let fts_mismatch: u32 = conn.query_row(
        "SELECT count(*) FROM (SELECT s.note_id FROM note_search s LEFT JOIN notes n ON \
            n.id=s.note_id GROUP BY s.note_id HAVING count(*)!=1 OR count(n.id)!=1 OR \
            max(s.front_text IS NOT n.front_text OR s.back_text IS NOT n.back_text))",
        [],
        |r| r.get(0),
    )?;
    let counts_match: bool = conn.query_row(
        "SELECT (SELECT count(*) FROM notes)=(SELECT count(*) FROM note_search)",
        [],
        |r| r.get(0),
    )?;
    if fts_mismatch > 0 || !counts_match {
        issues.push("The full-text search index needs rebuilding.".into());
    }
    for row in conn
        .prepare("SELECT id,settings,cover,color,name FROM decks")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
    {
        let (id, settings, cover, color, name) = row?;
        if name.trim().is_empty()
            || name.len() > 200
            || !["violet", "teal", "blue", "rose", "amber", "slate"].contains(&color.as_str())
            || !serde_json::from_str::<DeckSettings>(&settings)
                .is_ok_and(|settings| validate_settings(&settings).is_ok())
        {
            issues.push(format!("Invalid deck settings for {id}."));
        }
        if let Some(cover) = cover
            && (!valid_media_id(&cover)
                || !conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM media WHERE id=?1)",
                    [cover],
                    |r| r.get::<_, bool>(0),
                )?)
        {
            issues.push(format!("Missing cover reference for deck {id}."));
        }
    }
    for row in conn
        .prepare("SELECT name FROM tags")?
        .query_map([], |r| r.get::<_, String>(0))?
    {
        let tag = row?;
        if !tag_name(&tag).is_ok_and(|value| value == tag) {
            issues.push("A tag has an invalid name.".into());
        }
    }
    let mut missing_media = Vec::new();
    for row in conn
        .prepare("SELECT key,value FROM metadata WHERE key IN ('preferences','session')")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
    {
        let (key, value) = row?;
        if value.trim() == "null" {
            continue;
        }
        let valid = if key == "preferences" {
            serde_json::from_str::<Preferences>(&value).is_ok_and(|p| {
                validate_settings(&p.defaults).is_ok()
                    && [90, 100, 110, 125].contains(&p.scale)
                    && (1..=100).contains(&p.backup_retention)
            })
        } else {
            serde_json::from_str::<SessionRecord>(&value).is_ok_and(|s| {
                let ids: std::collections::HashSet<_> = s.card_ids.iter().collect();
                chrono::NaiveDate::parse_from_str(&s.day, "%Y-%m-%d").is_ok()
                    && s.card_ids.len() <= 500_000
                    && ids.len() == s.card_ids.len()
                    && s.completed
                        .iter()
                        .chain(s.skipped.iter())
                        .all(|id| ids.contains(id))
            })
        };
        if !valid {
            issues.push(format!("Invalid {key} metadata."));
        }
    }
    for row in conn
        .prepare("SELECT data FROM undo")?
        .query_map([], |r| r.get::<_, String>(0))?
    {
        if !serde_json::from_str::<UndoRecord>(&row?).is_ok_and(|undo| {
            undo.cards.len() <= 50_000
                && undo.cards.iter().all(|card| {
                    valid_schedule(&card.schedule)
                        && card.tags.iter().all(|tag| tag_name(tag).is_ok())
                })
        }) {
            issues.push("Invalid undo metadata.".into());
        }
    }
    for row in conn
        .prepare("SELECT id,before_state,after_state,day,timestamp,duration_ms FROM reviews")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?
    {
        let (id, before, after, day, time, duration) = row?;
        if !serde_json::from_str::<Schedule>(&before).is_ok_and(|s| valid_schedule(&s))
            || !serde_json::from_str::<Schedule>(&after).is_ok_and(|s| valid_schedule(&s))
            || !(0..=32_503_680_000).contains(&time)
            || !(0..=86_400_000).contains(&duration)
            || chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").is_err()
        {
            issues.push(format!("Invalid review history record {id}."));
        }
    }
    for row in conn
        .prepare("SELECT id,bytes FROM media")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))?
    {
        let (id, length) = row?;
        let path = media_dir.join(&id);
        if !valid_media_id(&id) || !path.is_file() {
            missing_media.push(id);
            continue;
        }
        if length > 20 * 1024 * 1024 || fs::metadata(&path)?.len() != length {
            issues.push(format!("Image size does not match its record: {id}."));
            continue;
        }
        let bytes = fs::read(path)?;
        if !id.starts_with(&hex::encode(Sha256::digest(&bytes))) {
            issues.push(format!("Image checksum failed: {id}."));
        }
        let valid_image = (|| -> Option<()> {
            let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()
                .ok()?;
            let format = reader.format()?;
            let extension = match format {
                image::ImageFormat::Png => "png",
                image::ImageFormat::Jpeg => "jpg",
                image::ImageFormat::WebP => "webp",
                image::ImageFormat::Gif => "gif",
                _ => return None,
            };
            if !id.ends_with(&format!(".{extension}")) {
                return None;
            }
            let (width, height) = reader.into_dimensions().ok()?;
            if width == 0 || height == 0 || width as u64 * height as u64 > 40_000_000 {
                return None;
            }
            image::load_from_memory_with_format(&bytes, format).ok()?;
            Some(())
        })()
        .is_some();
        if !valid_image {
            issues.push(format!("Image cannot be decoded safely: {id}."));
        }
    }
    let unused_media = conn
        .prepare(
            "SELECT id FROM media WHERE NOT EXISTS(SELECT 1 FROM note_media WHERE media_id=media.id) \
                AND NOT EXISTS(SELECT 1 FROM decks WHERE cover=media.id)",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(IntegrityReport {
        healthy: issues.is_empty() && missing_media.is_empty(),
        issues,
        missing_media,
        unused_media,
    })
}

impl Store {
    /// Only derived data is repaired; note content, schedules, and review history remain untouched.
    pub fn repair_indexes(&mut self) -> Result<IntegrityReport> {
        validate_schema(&self.conn)?;
        self.create_backup(false)?;
        self.transaction(|store| {
            let notes = store
                .conn
                .prepare("SELECT id,front,back FROM notes")?
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (id, front, back) in notes {
                let (front, mut media) = validate_document(&serde_json::from_str(&front)?)?;
                let (back, back_media) = validate_document(&serde_json::from_str(&back)?)?;
                media.extend(back_media);
                media.sort();
                media.dedup();
                for id in &media {
                    store.assert_media(id)?;
                }
                let front = plain_text(&front);
                let back = plain_text(&back);
                store.conn.execute("UPDATE notes SET front_text=?1,back_text=?2,front_key=?3 WHERE id=?4", params![front, back, normalized(&front), id])?;
                store.conn.execute("DELETE FROM note_media WHERE note_id=?1", [&id])?;
                for media in media {
                    store.conn.execute("INSERT INTO note_media(note_id,media_id) VALUES (?1,?2)", params![id, media])?;
                }
            }
            store.conn.execute_batch(
                "DELETE FROM note_search; INSERT INTO note_search(note_id,front_text,back_text) SELECT \
                    id,front_text,back_text FROM notes;",
            )?;
            store.dirty()
        })?;
        self.integrity()
    }
}
