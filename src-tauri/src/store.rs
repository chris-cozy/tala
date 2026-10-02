//! SQLite collection ownership, migrations, CRUD, and reversible bulk mutations.
//! Content and schedules have separate lifecycles: editing a note must not reset its card.

use crate::{
    clock::Clock,
    content::{normalized, plain_text, tag_name, validate_document},
    error::{AppError, Result},
    models::*,
    scheduler::validate_settings,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub struct Grant {
    pub path: PathBuf,
    pub purpose: String,
}
/// One open collection. The native API serializes access; backups identify this instance
/// so an older background job cannot acknowledge changes in a restored collection.
pub struct Store {
    pub conn: Connection,
    pub root: PathBuf,
    pub clock_override: Option<Clock>,
    pub grants: HashMap<String, Grant>,
    pub backup_warning: Option<String>,
    pub instance_id: String,
}

pub const CARD_SELECT: &str = "SELECT \
    c.id,c.note_id,c.deck_id,d.name,n.front,n.back,n.front_text,n.back_text,n.behavior,COALESCE((SELECT \
    json_group_array(tag) FROM note_tags WHERE \
    note_id=n.id),'[]'),c.schedule,c.suspended,c.buried_until,c.leech,n.created_at,n.modified_at,c.revision,c.deleted_at \
    FROM cards c JOIN notes n ON n.id=c.note_id JOIN decks d ON d.id=c.deck_id";
pub fn json_column<T: DeserializeOwned>(row: &Row<'_>, index: usize) -> rusqlite::Result<T> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}
pub fn map_card(row: &Row<'_>) -> rusqlite::Result<CardView> {
    let behavior: String = row.get(8)?;
    Ok(CardView {
        id: row.get(0)?,
        note_id: row.get(1)?,
        deck_id: row.get(2)?,
        deck_name: row.get(3)?,
        front: json_column(row, 4)?,
        back: json_column(row, 5)?,
        front_text: row.get(6)?,
        back_text: row.get(7)?,
        behavior: serde_json::from_value(serde_json::Value::String(behavior)).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?,
        tags: json_column(row, 9)?,
        schedule: json_column(row, 10)?,
        suspended: row.get(11)?,
        buried_until: row.get(12)?,
        leech: row.get(13)?,
        created_at: row.get(14)?,
        modified_at: row.get(15)?,
        revision: row.get(16)?,
        deleted_at: row.get(17)?,
    })
}
pub fn id() -> String {
    Uuid::new_v4().to_string()
}
pub fn behavior_text(b: &Behavior) -> &'static str {
    match b {
        Behavior::Normal => "normal",
        Behavior::Reversed => "reversed",
        Behavior::Typed => "typed",
    }
}

impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Self::recover_restore(&root)?;
        fs::create_dir_all(root.join("collection/media"))?;
        fs::create_dir_all(root.join("backups"))?;
        fs::create_dir_all(root.join("logs"))?;
        let mut conn = Connection::open(root.join("collection/tala.sqlite3"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        crate::hierarchy::migrate(&mut conn)?;
        Ok(Self {
            conn,
            root,
            clock_override: None,
            grants: HashMap::new(),
            backup_warning: None,
            instance_id: id(),
        })
    }
    pub fn now(&self) -> Clock {
        self.clock_override.unwrap_or_else(Clock::system)
    }
    pub fn media_dir(&self) -> PathBuf {
        self.root.join("collection/media")
    }
    pub fn metadata<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let value: Option<String> = self
            .conn
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        value
            .filter(|s| s.trim() != "null")
            .map(|s| serde_json::from_str(&s).map_err(AppError::from))
            .transpose()
    }
    pub fn set_metadata<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.conn.execute("INSERT INTO metadata(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn dirty(&self) -> Result<()> {
        self.set_metadata("changed_at", &self.now().now)?;
        self.set_metadata(
            "change_counter",
            &(self.metadata::<u64>("change_counter")?.unwrap_or(0) + 1),
        )
    }
    pub fn preferences(&self) -> Result<Preferences> {
        Ok(self.metadata("preferences")?.unwrap_or_default())
    }
    pub fn save_preferences(&mut self, p: Preferences) -> Result<()> {
        validate_settings(&p.defaults)?;
        if ![90, 100, 110, 125].contains(&p.scale) || !(1..=100).contains(&p.backup_retention) {
            return Err(AppError::invalid(
                "Choose a supported interface scale and retain 1–100 automatic backups.",
            ));
        }
        self.set_metadata("preferences", &p)?;
        self.dirty()
    }
    /// Savepoints let import and deck operations reuse transactional note/bulk mutations.
    /// An inner success must still roll back if the enclosing operation fails.
    pub fn transaction<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.conn.execute_batch("SAVEPOINT tala_change")?;
        match f(self) {
            Ok(result) => {
                self.conn.execute_batch("RELEASE tala_change")?;
                Ok(result)
            }
            Err(error) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO tala_change; RELEASE tala_change");
                Err(error)
            }
        }
    }
    pub fn clear_undo(&self) -> Result<()> {
        self.conn.execute("DELETE FROM undo", [])?;
        Ok(())
    }
    pub fn save_undo(&self, value: &UndoRecord) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO undo(singleton,data) VALUES (1,?1)",
            [serde_json::to_string(value)?],
        )?;
        Ok(())
    }
    pub fn undo_record(&self) -> Result<Option<UndoRecord>> {
        Ok(self
            .conn
            .query_row("SELECT data FROM undo WHERE singleton=1", [], |r| {
                json_column(r, 0)
            })
            .optional()?)
    }
    pub fn tags(&self) -> Result<Vec<String>> {
        Ok(self
            .conn
            .prepare("SELECT name FROM tags ORDER BY name")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn set_tags(&self, note_id: &str, tags: &[String]) -> Result<()> {
        if tags.len() > 100 {
            return Err(AppError::invalid("A note can have up to 100 tags."));
        }
        self.conn
            .execute("DELETE FROM note_tags WHERE note_id=?1", [note_id])?;
        for raw in tags {
            let name = tag_name(raw)?;
            self.conn
                .execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [&name])?;
            self.conn.execute(
                "INSERT OR IGNORE INTO note_tags(note_id,tag) VALUES (?1,?2)",
                params![note_id, name],
            )?;
        }
        Ok(())
    }
    pub fn edit_tag(&mut self, from: &str, to: Option<String>) -> Result<()> {
        self.transaction(|store| {
            store.clear_undo()?;
            store.conn.execute("UPDATE notes SET modified_at=?1 WHERE id IN (SELECT note_id FROM note_tags WHERE tag=?2)", params![store.now().now, from])?;
            match to {
                Some(to) => {
                    let name = tag_name(&to)?;
                    store.conn.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [&name])?;
                    store.conn.execute("INSERT OR IGNORE INTO note_tags(note_id,tag) SELECT note_id,?1 FROM note_tags WHERE tag=?2", params![name, from])?;
                    if name != from {
                        store.conn.execute("DELETE FROM tags WHERE name=?1", [from])?;
                    }
                }
                None => {
                    store.conn.execute("DELETE FROM tags WHERE name=?1", [from])?;
                }
            }
            store.dirty()
        })
    }
    pub fn assert_deck(&self, deck: &str) -> Result<()> {
        if !self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM decks WHERE id=?1 AND deleted_at IS NULL)",
            [deck],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(AppError::invalid("Choose an existing deck."));
        }
        Ok(())
    }
    pub fn deck_settings(&self, deck: &str) -> Result<DeckSettings> {
        Ok(self
            .conn
            .query_row("SELECT settings FROM decks WHERE id=?1", [deck], |r| {
                json_column(r, 0)
            })?)
    }
    pub fn save_deck(&mut self, input: DeckInput) -> Result<String> {
        let name = input.name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err(AppError::invalid(
                "Deck names must contain 1–200 characters.",
            ));
        }
        validate_settings(&input.settings)?;
        if !["violet", "teal", "blue", "rose", "amber", "slate"].contains(&input.color.as_str()) {
            return Err(AppError::invalid("Choose a supported deck color."));
        }
        if let Some(ref cover) = input.cover {
            self.assert_media_kind(cover, "image/")?;
        }
        if let Some(parent) = &input.parent_id {
            self.assert_deck(parent)?;
            if let Some(id) = &input.id
                && (id == parent || self.descendant_ids(id)?.contains(parent))
            {
                return Err(AppError::invalid(
                    "A deck cannot be placed inside itself or one of its descendants.",
                ));
            }
        }
        let deck_id = input.id.clone().unwrap_or_else(id);
        let now = self.now().now;
        self.transaction(|store| {
            store.clear_undo()?;
            if input.id.is_some() {
                store.assert_deck(&deck_id)?;
                store
                    .conn
                    .execute("UPDATE decks SET name=?1,cover=?2,color=?3,settings=?4,updated_at=?5,parent_id=?7 WHERE id=?6", params![name, input.cover, input.color, serde_json::to_string(&input.settings)?, now, deck_id, input.parent_id])?;
            } else {
                store.conn.execute(
                    "INSERT INTO decks(id,name,cover,color,settings,created_at,updated_at,parent_id) VALUES (?1,?2,?3,?4,?5,?6,?6,?7)",
                    params![deck_id, name, input.cover, input.color, serde_json::to_string(&input.settings)?, now, input.parent_id],
                )?;
            }
            store.dirty()?;
            Ok(deck_id.clone())
        })
    }
    pub fn card(&self, card_id: &str) -> Result<CardView> {
        self.conn
            .query_row(&format!("{CARD_SELECT} WHERE c.id=?1"), [card_id], map_card)
            .optional()?
            .ok_or_else(|| AppError::invalid("This card no longer exists."))
    }
    pub fn note_card(&self, note_id: &str) -> Result<CardView> {
        self.conn
            .query_row(&format!("{CARD_SELECT} WHERE n.id=?1"), [note_id], map_card)
            .optional()?
            .ok_or_else(|| AppError::invalid("This note no longer exists."))
    }
    pub fn assert_media(&self, media: &str) -> Result<()> {
        if !crate::content::valid_media_id(media)
            || !self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM media WHERE id=?1)",
                [media],
                |r| r.get::<_, bool>(0),
            )?
        {
            return Err(AppError::invalid(
                "An image reference is not part of this collection. Attach the image again.",
            ));
        }
        Ok(())
    }
    pub(crate) fn assert_media_kind(&self, id: &str, kind: &str) -> Result<()> {
        self.assert_media(id)?;
        let mime: String =
            self.conn
                .query_row("SELECT mime FROM media WHERE id=?1", [id], |r| r.get(0))?;
        if !mime.starts_with(kind) {
            return Err(AppError::invalid(
                "This media reference has the wrong type. Attach the file again.",
            ));
        }
        Ok(())
    }
    pub(crate) fn assert_document_media(&self, document: &serde_json::Value) -> Result<()> {
        match document["type"].as_str() {
            Some("audio") => self.assert_media_kind(
                document["attrs"]["mediaId"].as_str().unwrap_or(""),
                "audio/",
            )?,
            Some("image") => self.assert_media_kind(
                document["attrs"]["mediaId"].as_str().unwrap_or(""),
                "image/",
            )?,
            _ => {}
        }
        if let Some(children) = document["content"].as_array() {
            for node in children {
                self.assert_document_media(node)?;
            }
        }
        Ok(())
    }
    /// Updates content, tags, and deck membership while preserving memory and review history.
    pub fn save_note(&mut self, input: NoteInput) -> Result<CardView> {
        self.assert_deck(&input.deck_id)?;
        let (front, mut media) = validate_document(&input.front)?;
        let (back, back_media) = validate_document(&input.back)?;
        media.extend(back_media);
        self.assert_document_media(&front)?;
        self.assert_document_media(&back)?;
        let front_text = plain_text(&front);
        let back_text = plain_text(&back);
        let key = normalized(&front_text);
        let note_id = input.id.clone().unwrap_or_else(id);
        let now = self.now().now;
        self.transaction(|store| {
            store.clear_undo()?;
            if input.id.is_some() {
                let old = store.note_card(&note_id)?;
                if old.deleted_at.is_some() {
                    return Err(AppError::invalid("Restore the note before editing it."));
                }
                store.conn.execute(
                    "UPDATE notes SET front=?1,back=?2,front_text=?3,back_text=?4,front_key=?5,behavior=?6,modified_at=?7 WHERE id=?8",
                    params![front.to_string(), back.to_string(), front_text, back_text, key, behavior_text(&input.behavior), now, note_id],
                )?;
                store.conn.execute("UPDATE cards SET deck_id=?1,revision=revision+1 WHERE note_id=?2", params![input.deck_id, note_id])?;
            } else {
                store.conn.execute(
                    "INSERT INTO \
                        notes(id,front,back,front_text,back_text,front_key,behavior,created_at,modified_at) \
                        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
                    params![note_id, front.to_string(), back.to_string(), front_text, back_text, key, behavior_text(&input.behavior), now],
                )?;
                store
                    .conn
                    .execute("INSERT INTO cards(id,note_id,deck_id,phase,due,schedule) VALUES (?1,?2,?3,'new',?4,?5)", params![id(), note_id, input.deck_id, now, serde_json::to_string(&Schedule::new(now))?])?;
            }
            store.set_tags(&note_id, &input.tags)?;
            store.conn.execute("DELETE FROM note_media WHERE note_id=?1", [&note_id])?;
            for media_id in media {
                store.conn.execute("INSERT OR IGNORE INTO note_media(note_id,media_id) VALUES (?1,?2)", params![note_id, media_id])?;
            }
            store.dirty()?;
            store.note_card(&note_id)
        })
    }
    /// Keep indexed phase/due columns in sync; bump the revision so stale grades are rejected.
    pub fn write_schedule(&self, card_id: &str, schedule: &Schedule) -> Result<()> {
        self.conn.execute(
            "UPDATE cards SET schedule=?1,phase=?2,due=?3,revision=revision+1 WHERE id=?4",
            params![
                serde_json::to_string(schedule)?,
                schedule.phase.as_str(),
                schedule.due,
                card_id
            ],
        )?;
        Ok(())
    }
    pub fn browse(&self, query: BrowseQuery) -> Result<CardPage> {
        let mut clauses = vec![
            if query.trash {
                "c.deleted_at IS NOT NULL"
            } else {
                "c.deleted_at IS NULL AND d.deleted_at IS NULL"
            }
            .to_string(),
        ];
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if !query.search.trim().is_empty() {
            let terms = query
                .search
                .split_whitespace()
                .map(|s| format!("\"{}\"*", s.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" AND ");
            clauses
                .push("n.id IN (SELECT note_id FROM note_search WHERE note_search MATCH ?)".into());
            values.push(terms.into());
        }
        if let Some(deck) = query.deck {
            let ids = if query.only_this_deck {
                vec![deck]
            } else {
                self.descendant_ids(&deck)?
            };
            clauses.push("c.deck_id IN (SELECT value FROM json_each(?))".into());
            values.push(serde_json::to_string(&ids)?.into());
        }
        if let Some(tag) = query.tag {
            clauses.push(
                "EXISTS(SELECT 1 FROM note_tags nt WHERE nt.note_id=n.id AND nt.tag=?)".into(),
            );
            values.push(tag.into());
        }
        if let Some(state) = query.state {
            match state.as_str() {
                "suspended" => clauses.push("c.suspended=1".into()),
                "buried" => {
                    clauses.push("c.suspended=0 AND c.buried_until>?".into());
                    values.push(self.now().now.into());
                }
                "new" | "learning" | "review" | "relearning" => {
                    clauses.push("c.phase=? AND c.suspended=0 AND (c.buried_until IS NULL OR c.buried_until<=?)".into());
                    values.push(state.into());
                    values.push(self.now().now.into());
                }
                _ => return Err(AppError::invalid("Unknown card state.")),
            }
        }
        if query.leech {
            clauses.push("c.leech=1".into());
        }
        let condition = clauses.join(" AND ");
        let total = self
            .conn
            .query_row(&format!("SELECT count(*) FROM cards c JOIN notes n ON n.id=c.note_id JOIN decks d ON d.id=c.deck_id WHERE {condition}"), rusqlite::params_from_iter(values.iter()), |r| r.get(0))?;
        let sort = match query.sort.as_str() {
            "front" => "n.front_text COLLATE NOCASE",
            "deck" => "d.name COLLATE NOCASE",
            "state" => "c.phase",
            "due" => "c.due",
            "reviews" => "json_extract(c.schedule,'$.reviewCount')",
            "lapses" => "json_extract(c.schedule,'$.lapses')",
            _ => "n.created_at",
        };
        let dir = if query.descending { "DESC" } else { "ASC" };
        values.push(
            (if query.limit == 0 {
                100
            } else {
                query.limit.min(500)
            } as i64)
                .into(),
        );
        values.push((query.offset as i64).into());
        let sql = format!(
            "{CARD_SELECT} WHERE {condition} ORDER BY {sort} {dir},n.rowid LIMIT ? OFFSET ?"
        );
        let cards = self
            .conn
            .prepare(&sql)?
            .query_map(rusqlite::params_from_iter(values.iter()), map_card)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(CardPage { cards, total })
    }
    pub fn bulk(&mut self, input: BulkInput) -> Result<u32> {
        if input.ids.is_empty() || input.ids.len() > 50_000 {
            return Err(AppError::invalid("Select between 1 and 50,000 cards."));
        }
        let ids: std::collections::BTreeSet<_> = input.ids.into_iter().collect();
        let cards = ids
            .iter()
            .map(|id| self.card(id))
            .collect::<Result<Vec<_>>>()?;
        if input.action == "move" || input.action == "restore" {
            if let Some(ref deck) = input.value {
                self.assert_deck(deck)?;
            } else if input.action == "move" {
                return Err(AppError::invalid("Choose a destination deck."));
            }
        }
        let clock = self.now();
        self.transaction(|store| {
            if input.action == "purge" {
                store.clear_undo()?;
            } else {
                store.save_undo(&UndoRecord {
                    label: format!("{} {} card{}", input.action, cards.len(), if cards.len() == 1 { "" } else { "s" }),
                    cards: cards.clone(),
                    session: store.metadata("session")?,
                    review_id: None,
                })?;
            }
            for card in &cards {
                match input.action.as_str() {
                    "suspend" | "unsuspend" => {
                        store.conn.execute("UPDATE cards SET suspended=?1,revision=revision+1 WHERE id=?2", params![input.action == "suspend", card.id])?;
                    }
                    "bury" | "unbury" => {
                        store.conn.execute("UPDATE cards SET buried_until=?1,revision=revision+1 WHERE id=?2", params![if input.action == "bury" { Some(clock.after_days(1)) } else { None }, card.id])?;
                    }
                    "move" => {
                        store.conn.execute("UPDATE cards SET deck_id=?1,revision=revision+1 WHERE id=?2", params![input.value, card.id])?;
                    }
                    "add_tag" | "remove_tag" => {
                        let tag = tag_name(input.value.as_deref().unwrap_or(""))?;
                        let mut tags = card.tags.clone();
                        if input.action == "add_tag" {
                            tags.push(tag)
                        } else {
                            tags.retain(|x| x != &tag)
                        };
                        store.set_tags(&card.note_id, &tags)?;
                        store.conn.execute("UPDATE notes SET modified_at=?1 WHERE id=?2", params![clock.now, card.note_id])?;
                    }
                    "delete" => {
                        store.conn.execute("UPDATE cards SET deleted_at=?1,revision=revision+1 WHERE id=?2", params![clock.now, card.id])?;
                        store.conn.execute("UPDATE notes SET deleted_at=?1 WHERE id=?2", params![clock.now, card.note_id])?;
                    }
                    "restore" => {
                        let deck = input.value.as_deref().unwrap_or(&card.deck_id);
                        store.assert_deck(deck)?;
                        store.conn.execute("UPDATE cards SET deleted_at=NULL,deck_id=?1,revision=revision+1 WHERE id=?2", params![deck, card.id])?;
                        store.conn.execute("UPDATE notes SET deleted_at=NULL WHERE id=?1", [&card.note_id])?;
                    }
                    "purge" => {
                        if card.deleted_at.is_none() {
                            return Err(AppError::invalid("Only notes in Recently Deleted can be permanently removed."));
                        }
                        store.conn.execute("DELETE FROM notes WHERE id=?1", [&card.note_id])?;
                    }
                    "reset" => {
                        let mut s = Schedule::new(clock.now);
                        s.review_count = card.schedule.review_count;
                        s.lapses = card.schedule.lapses;
                        store.write_schedule(&card.id, &s)?;
                    }
                    "set_due" => {
                        let due = input.value.as_deref().unwrap_or("").parse::<i64>().map_err(|_| AppError::invalid("Choose a valid due date."))?;
                        if !(0..=32_503_680_000).contains(&due) {
                            return Err(AppError::invalid("Choose a valid due date."));
                        }
                        let mut s = card.schedule.clone();
                        s.due = due;
                        store.write_schedule(&card.id, &s)?;
                    }
                    "reschedule" => {
                        if card.schedule.phase != Phase::Review {
                            continue;
                        }
                        let settings = store.deck_settings(&card.deck_id)?;
                        let mut s = card.schedule.clone();
                        let stability = s.stability.ok_or_else(|| AppError::invalid("This card has no valid memory state."))?;
                        let fsrs = fsrs::FSRS::default();
                        let days = (fsrs.next_interval(Some(stability), settings.retention, 0).round().max(1.0) as u32).min(settings.maximum_interval);
                        let last = s.last_review.unwrap_or(clock.now);
                        s.scheduled_days = days;
                        s.due = Clock::at(last, clock.zone).after_days(days).max(clock.day_start());
                        store.write_schedule(&card.id, &s)?;
                    }
                    _ => return Err(AppError::invalid("Unsupported bulk action.")),
                }
            }
            store.dirty()?;
            Ok(cards.len() as u32)
        })
    }
    pub fn delete_deck(&mut self, deck: &str, move_to: Option<String>) -> Result<()> {
        self.delete_deck_scope(deck, move_to, false)
    }
    pub fn delete_deck_scope(
        &mut self,
        deck: &str,
        move_to: Option<String>,
        branch: bool,
    ) -> Result<()> {
        self.assert_deck(deck)?;
        let descendants = self.descendant_ids(deck)?;
        if let Some(ref target) = move_to {
            self.assert_deck(target)?;
            if descendants.contains(target) {
                return Err(AppError::invalid(
                    "Choose a destination outside this branch.",
                ));
            }
        }
        let ids = if branch {
            descendants
        } else {
            vec![deck.to_string()]
        };
        let parent: Option<String> =
            self.conn
                .query_row("SELECT parent_id FROM decks WHERE id=?1", [deck], |r| {
                    r.get(0)
                })?;
        self.transaction(|store| {
            let cards = store.conn.prepare("SELECT id FROM cards WHERE deck_id IN (SELECT value FROM json_each(?1)) AND deleted_at IS NULL")?
                .query_map([serde_json::to_string(&ids)?], |r|r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            if !cards.is_empty() {
                store.bulk(BulkInput{ids:cards, action:if move_to.is_some(){"move"}else{"delete"}.into(), value:move_to})?;
            }
            if !branch {
                store.conn.execute("UPDATE decks SET parent_id=?1,updated_at=?2 WHERE parent_id=?3 AND deleted_at IS NULL", params![parent,store.now().now,deck])?;
            }
            store.clear_undo()?;
            store.conn.execute("UPDATE decks SET deleted_at=?1 WHERE id IN (SELECT value FROM json_each(?2))",params![store.now().now,serde_json::to_string(&ids)?])?;
            store.dirty()
        })
    }
    pub fn undo(&mut self) -> Result<String> {
        let record = self
            .undo_record()?
            .ok_or_else(|| AppError::invalid("There is nothing to undo."))?;
        self.transaction(|store| {
            for card in &record.cards {
                store.conn.execute(
                    "UPDATE cards SET deck_id=?1,suspended=?2,buried_until=?3,leech=?4,deleted_at=?5 WHERE id=?6",
                    params![card.deck_id, card.suspended, card.buried_until, card.leech, card.deleted_at, card.id],
                )?;
                store.write_schedule(&card.id, &card.schedule)?;
                store.conn.execute("UPDATE notes SET deleted_at=?1,modified_at=?2 WHERE id=?3", params![card.deleted_at, card.modified_at, card.note_id])?;
                store.set_tags(&card.note_id, &card.tags)?;
            }
            if let Some(review) = &record.review_id {
                store.conn.execute("UPDATE reviews SET undone_at=?1 WHERE id=?2 AND undone_at IS NULL", params![store.now().now, review])?;
                store.conn.execute("INSERT INTO review_reversals(id,review_id,timestamp) VALUES (?1,?2,?3)", params![id(), review, store.now().now])?;
            }
            store.set_metadata("session", &record.session)?;
            store.clear_undo()?;
            store.dirty()?;
            Ok(record.label)
        })
    }
}
