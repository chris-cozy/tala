//! Shared storage and IPC models. ts-rs exports TypeScript contracts during tests;
//! edit these Rust definitions rather than the generated bindings. Timestamps are UTC seconds.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum Behavior {
    Normal,
    Reversed,
    Typed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum Phase {
    New,
    Learning,
    Review,
    Relearning,
}
impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Learning => "learning",
            Self::Review => "review",
            Self::Relearning => "relearning",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Schedule {
    pub phase: Phase,
    #[ts(type = "number")]
    pub due: i64,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub scheduled_days: u32,
    pub review_count: u32,
    pub lapses: u32,
    #[ts(type = "number | null")]
    pub last_review: Option<i64>,
    pub step: u32,
}
impl Schedule {
    pub fn new(now: i64) -> Self {
        Self {
            phase: Phase::New,
            due: now,
            stability: None,
            difficulty: None,
            scheduled_days: 0,
            review_count: 0,
            lapses: 0,
            last_review: None,
            step: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DeckSettings {
    pub new_limit: u32,
    pub review_limit: u32,
    pub retention: f32,
    pub learning_steps: Vec<u32>,
    pub relearning_steps: Vec<u32>,
    pub maximum_interval: u32,
    pub new_order: String,
    pub review_order: String,
    pub new_placement: String,
    pub leech_threshold: u32,
    pub suspend_leeches: bool,
}
impl Default for DeckSettings {
    fn default() -> Self {
        Self {
            new_limit: 20,
            review_limit: 200,
            retention: 0.9,
            learning_steps: vec![60, 600],
            relearning_steps: vec![600],
            maximum_interval: 36_500,
            new_order: "created".into(),
            review_order: "due".into(),
            new_placement: "after".into(),
            leech_threshold: 8,
            suspend_leeches: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Preferences {
    pub defaults: DeckSettings,
    pub scale: u32,
    pub backup_enabled: bool,
    pub backup_retention: u32,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            defaults: DeckSettings::default(),
            scale: 100,
            backup_enabled: true,
            backup_retention: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Deck {
    pub id: String,
    pub name: String,
    pub cover: Option<String>,
    pub color: String,
    pub settings: DeckSettings,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    pub total: u32,
    pub new_count: u32,
    pub learning: u32,
    pub review: u32,
    pub due: u32,
    pub in_review: u32,
    pub limited_new: u32,
    pub limited_review: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DeckInput {
    pub id: Option<String>,
    pub name: String,
    pub cover: Option<String>,
    pub color: String,
    pub settings: DeckSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct NoteInput {
    pub id: Option<String>,
    pub deck_id: String,
    pub front: Value,
    pub back: Value,
    pub behavior: Behavior,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct CardView {
    pub id: String,
    pub note_id: String,
    pub deck_id: String,
    pub deck_name: String,
    pub front: Value,
    pub back: Value,
    pub front_text: String,
    pub back_text: String,
    pub behavior: Behavior,
    pub tags: Vec<String>,
    pub schedule: Schedule,
    pub suspended: bool,
    #[ts(type = "number | null")]
    pub buried_until: Option<i64>,
    pub leech: bool,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub modified_at: i64,
    pub revision: u32,
    #[ts(type = "number | null")]
    pub deleted_at: Option<i64>,
}
impl CardView {
    pub fn effective_state(&self, now: i64) -> &str {
        if self.suspended {
            "suspended"
        } else if self.buried_until.is_some_and(|x| x > now) {
            "buried"
        } else {
            self.schedule.phase.as_str()
        }
    }
    pub fn eligible(&self, now: i64) -> bool {
        self.deleted_at.is_none() && !self.suspended && self.buried_until.is_none_or(|x| x <= now)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BrowseQuery {
    pub search: String,
    pub deck: Option<String>,
    pub tag: Option<String>,
    pub state: Option<String>,
    pub leech: bool,
    pub trash: bool,
    pub sort: String,
    pub descending: bool,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct CardPage {
    pub cards: Vec<CardView>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BulkInput {
    pub ids: Vec<String>,
    pub action: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
/// Stable membership makes progress meaningful even when cards repeat or become ineligible.
pub struct SessionRecord {
    pub id: String,
    pub day: String,
    pub deck_id: Option<String>,
    pub card_ids: Vec<String>,
    pub completed: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct GradeOption {
    pub grade: u8,
    pub label: String,
    pub interval: String,
    pub schedule: Schedule,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct StudyView {
    pub session: SessionRecord,
    pub card: Option<CardView>,
    pub options: Vec<GradeOption>,
    #[ts(type = "number | null")]
    pub next_due: Option<i64>,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ReviewInput {
    pub session_id: String,
    pub card_id: String,
    pub revision: u32,
    pub operation_id: String,
    pub grade: u8,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ReviewEvent {
    pub id: String,
    pub card_id: String,
    pub deck_name: String,
    #[ts(type = "number")]
    pub timestamp: i64,
    pub day: String,
    pub grade: u8,
    pub before: Schedule,
    pub after: Schedule,
    pub duration_ms: u32,
    #[ts(type = "number | null")]
    pub undone_at: Option<i64>,
    pub scheduler: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct PeriodSummary {
    pub reviewed: u32,
    pub seconds: f64,
    pub recall: f64,
    pub learned: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DayStat {
    pub day: String,
    pub count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct StateStat {
    pub name: String,
    pub count: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Statistics {
    pub summary: PeriodSummary,
    pub previous: PeriodSummary,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub days: Vec<DayStat>,
    pub grades: Vec<StateStat>,
    pub states: Vec<StateStat>,
    pub decks: Vec<Deck>,
    pub forecast: Vec<DayStat>,
    pub estimated_retention: Option<f64>,
    pub average_stability: Option<f64>,
    pub average_difficulty: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Today {
    pub new_count: u32,
    pub learning: u32,
    pub review: u32,
    pub studied: u32,
    pub streak: u32,
    #[ts(type = "number | null")]
    pub next_due: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Bootstrap {
    pub decks: Vec<Deck>,
    pub tags: Vec<String>,
    pub preferences: Preferences,
    pub today: Today,
    pub media_dir: String,
    pub data_dir: String,
    pub backup_warning: Option<String>,
    pub undo_label: Option<String>,
    pub session: Option<SessionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoRecord {
    pub label: String,
    pub cards: Vec<CardView>,
    pub session: Option<SessionRecord>,
    pub review_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImportConfig {
    pub path_token: String,
    pub delimiter: String,
    pub has_header: bool,
    pub front_column: u32,
    pub back_column: u32,
    pub tags_column: Option<u32>,
    pub deck_id: String,
    pub behavior: Behavior,
    pub duplicates: String,
    pub preview_digest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImportRow {
    pub line: u32,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub duplicate: bool,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImportPreview {
    pub rows: Vec<ImportRow>,
    pub total: u32,
    pub duplicates: u32,
    pub errors: Vec<String>,
    pub headers: Vec<String>,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ImportResult {
    pub imported: u32,
    pub updated: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct BackupInfo {
    pub name: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub bytes: u64,
    pub automatic: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct IntegrityReport {
    pub healthy: bool,
    pub issues: Vec<String>,
    pub missing_media: Vec<String>,
    pub unused_media: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub struct FileSelection {
    pub token: String,
    pub name: String,
}
