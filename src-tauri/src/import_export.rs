//! Delimited text transfers with explicit mapping and duplicate handling.
//! A preview digest binds approval to both file bytes and import configuration.

use crate::{
    content::{normalized, tag_name, text_document},
    error::{AppError, Result},
    files::atomic_write,
    models::*,
    store::{CARD_SELECT, Store, behavior_text, map_card},
};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs};

impl Store {
    fn parse_import(&self, config: &ImportConfig) -> Result<ImportPreview> {
        self.assert_deck(&config.deck_id)?;
        if !["skip", "update", "separate"].contains(&config.duplicates.as_str()) {
            return Err(AppError::invalid(
                "Choose how duplicates should be handled.",
            ));
        }
        let path = self.granted_path(&config.path_token, "import")?;
        if fs::metadata(&path)?.len() > 100 * 1024 * 1024 {
            return Err(AppError::invalid(
                "Import files must be smaller than 100 MB.",
            ));
        }
        let bytes = fs::read(path)?;
        let mut preview_config = config.clone();
        preview_config.preview_digest = None;
        let mut hash = Sha256::new();
        hash.update(&bytes);
        hash.update(serde_json::to_vec(&preview_config)?);
        let digest = hex::encode(hash.finalize());
        let delimiter = match config.delimiter.as_str() {
            "csv" => b',',
            "tsv" => b'\t',
            _ => return Err(AppError::invalid("Choose CSV or TSV.")),
        };
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .delimiter(delimiter)
            .flexible(true)
            .from_reader(bytes.as_slice());
        let mut rows = Vec::new();
        let mut headers = Vec::new();
        let mut errors = Vec::new();
        let mut seen = HashSet::new();
        let mut duplicates = 0;
        for (index, record) in reader.records().enumerate() {
            if index > 500_000 {
                return Err(AppError::invalid(
                    "Import files can contain up to 500,000 rows.",
                ));
            }
            let record = record?;
            if index == 0 {
                headers = if config.has_header {
                    record
                        .iter()
                        .map(|s| s.trim_start_matches('\u{feff}').to_string())
                        .collect()
                } else {
                    (1..=record.len()).map(|i| format!("Column {i}")).collect()
                };
                if config.has_header {
                    continue;
                }
            }
            let front = record
                .get(config.front_column as usize)
                .unwrap_or("")
                .trim_start_matches('\u{feff}')
                .to_string();
            let back = record
                .get(config.back_column as usize)
                .unwrap_or("")
                .to_string();
            let mut error = None;
            if front.trim().is_empty() || back.trim().is_empty() {
                error = Some("Front and Back are required; check the column mapping.".to_string());
            }
            if front.len() > 500_000 || back.len() > 500_000 {
                error = Some("A field is too long.".into());
            }
            let tags = if let Some(column) = config.tags_column {
                record
                    .get(column as usize)
                    .unwrap_or("")
                    .split(';')
                    .filter(|s| !s.trim().is_empty())
                    .map(tag_name)
                    .collect::<Result<Vec<_>>>()
            } else {
                Ok(vec![])
            };
            let tags = match tags {
                Ok(tags) => tags,
                Err(e) => {
                    error = Some(e.message);
                    vec![]
                }
            };
            if tags.len() > 100 {
                error = Some("A note can have at most 100 tags.".into());
            }
            let key = normalized(&front);
            let existing: u32 = self.conn.query_row(
                "SELECT count(*) FROM notes n JOIN cards c ON c.note_id=n.id WHERE n.front_key=?1 AND \
                    n.behavior=?2 AND c.deck_id=?3 AND c.deleted_at IS NULL",
                params![key, behavior_text(&config.behavior), config.deck_id],
                |r| r.get(0),
            )?;
            let duplicate = existing > 0 || !seen.insert(key);
            if duplicate {
                duplicates += 1;
            }
            if existing > 1 && config.duplicates == "update" {
                error = Some(
                    "More than one existing note matches. Choose Skip or Import separately.".into(),
                );
            }
            if let Some(ref error) = error
                && errors.len() < 100
            {
                errors.push(format!("Row {}: {error}", index + 1));
            }
            rows.push(ImportRow {
                line: index as u32 + 1,
                front,
                back,
                tags,
                duplicate,
                error,
            });
        }
        if headers.len() < 2 {
            errors.push("The file must have at least two columns.".into());
        }
        if rows.is_empty() {
            errors.push("The file contains no notes.".into());
        }
        Ok(ImportPreview {
            total: rows.len() as u32,
            rows,
            duplicates,
            errors,
            headers,
            digest,
        })
    }
    pub fn preview_import(&self, config: &ImportConfig) -> Result<ImportPreview> {
        let mut preview = self.parse_import(config)?;
        preview.rows.truncate(50);
        Ok(preview)
    }
    pub fn commit_import(&mut self, config: ImportConfig) -> Result<ImportResult> {
        let preview = self.parse_import(&config)?;
        if config.preview_digest.as_ref() != Some(&preview.digest) {
            return Err(AppError::conflict(
                "Preview the current file before importing. It may have changed since the last preview.",
            ));
        }
        if !preview.errors.is_empty() {
            return Err(AppError::invalid(format!(
                "Nothing was imported. {}",
                preview.errors[0]
            )));
        }
        self.transaction(|store| {
            let mut result = ImportResult { imported: 0, updated: 0, skipped: 0 };
            for row in preview.rows {
                let existing: Option<String> = store
                    .conn
                    .query_row(
                        "SELECT n.id FROM notes n JOIN cards c ON c.note_id=n.id WHERE n.front_key=?1 AND \
                            n.behavior=?2 AND c.deck_id=?3 AND c.deleted_at IS NULL LIMIT 1",
                        params![normalized(&row.front), behavior_text(&config.behavior), config.deck_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if existing.is_some() && config.duplicates == "skip" {
                    result.skipped += 1;
                    continue;
                }
                let note_id = if config.duplicates == "update" { existing } else { None };
                if note_id.is_some() {
                    result.updated += 1;
                } else {
                    result.imported += 1;
                }
                store.save_note(NoteInput {
                    id: note_id,
                    deck_id: config.deck_id.clone(),
                    front: text_document(&row.front),
                    back: text_document(&row.back),
                    behavior: config.behavior.clone(),
                    tags: row.tags,
                })?;
            }
            Ok(result)
        })
    }
    pub fn export_delimited(
        &self,
        token: &str,
        format: &str,
        deck: Option<String>,
        ids: Vec<String>,
    ) -> Result<u32> {
        self.export_delimited_scope(token, format, deck, ids, false)
    }
    pub fn export_delimited_scope(
        &self,
        token: &str,
        format: &str,
        deck: Option<String>,
        ids: Vec<String>,
        only_this_deck: bool,
    ) -> Result<u32> {
        let path = self.granted_path(token, "delimited")?;
        let delimiter = match format {
            "csv" => b',',
            "tsv" => b'\t',
            _ => return Err(AppError::invalid("Choose CSV or TSV.")),
        };
        let mut sql = format!("{CARD_SELECT} WHERE c.deleted_at IS NULL");
        let mut values = Vec::<rusqlite::types::Value>::new();
        if let Some(deck) = deck {
            sql.push_str(" AND c.deck_id IN (SELECT value FROM json_each(?))");
            let decks = if only_this_deck {
                vec![deck]
            } else {
                self.descendant_ids(&deck)?
            };
            values.push(serde_json::to_string(&decks)?.into());
        }
        if !ids.is_empty() {
            sql.push_str(" AND c.id IN (SELECT value FROM json_each(?))");
            values.push(serde_json::to_string(&ids)?.into());
        }
        sql.push_str(" ORDER BY n.created_at,c.id");
        let cards = self
            .conn
            .prepare(&sql)?
            .query_map(rusqlite::params_from_iter(values.iter()), map_card)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut writer = csv::WriterBuilder::new()
            .delimiter(delimiter)
            .from_writer(vec![]);
        writer.write_record(["Front", "Back", "Behavior", "Deck", "Tags"])?;
        let paths: std::collections::HashMap<_, _> = self
            .deck_records()?
            .into_iter()
            .map(|d| (d.id, d.path))
            .collect();
        for card in &cards {
            writer.write_record([
                card.front_text.as_str(),
                card.back_text.as_str(),
                behavior_text(&card.behavior),
                paths
                    .get(&card.deck_id)
                    .map(String::as_str)
                    .unwrap_or(&card.deck_name),
                card.tags.join(";").as_str(),
            ])?;
        }
        let bytes = writer
            .into_inner()
            .map_err(|_| AppError::storage("The export could not be finalized."))?;
        atomic_write(&path, &bytes)?;
        Ok(cards.len() as u32)
    }
}
