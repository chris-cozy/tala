//! Daily admission limits and fixed-cohort study sessions. Repeated learning steps
//! remain in the session, while new cards and later study days require a fresh cohort.

use crate::{
    error::{AppError, Result},
    models::*,
    scheduler,
    store::{Store, id, json_column},
};
use rusqlite::{OptionalExtension, params};
use std::collections::{HashMap, HashSet};

impl Store {
    pub fn daily_used(&self, deck: &str, category: &str) -> Result<u32> {
        Ok(self
            .conn
            .query_row("SELECT count(DISTINCT card_id) FROM reviews WHERE deck_id=?1 AND day=?2 AND category=?3 AND undone_at IS NULL", params![deck, self.now().day(), category], |r| r.get(0))?)
    }
    pub fn decks(&self) -> Result<Vec<Deck>> {
        let mut decks = self
            .conn
            .prepare(
                "SELECT id,name,cover,color,settings,created_at,updated_at FROM decks WHERE deleted_at IS \
                    NULL ORDER BY name COLLATE NOCASE,id",
            )?
            .query_map([], |r| {
                Ok(Deck {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    cover: r.get(2)?,
                    color: r.get(3)?,
                    settings: json_column(r, 4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                    total: 0,
                    new_count: 0,
                    learning: 0,
                    review: 0,
                    due: 0,
                    in_review: 0,
                    limited_new: 0,
                    limited_review: 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let now = self.now().now;
        for deck in &mut decks {
            deck.total = self.conn.query_row(
                "SELECT count(*) FROM cards WHERE deck_id=?1 AND deleted_at IS NULL",
                [&deck.id],
                |r| r.get(0),
            )?;
            let mut counts = HashMap::<String, u32>::new();
            let rows = self
                .conn
                .prepare(
                    "SELECT phase,count(*) FROM cards WHERE deck_id=?1 AND deleted_at IS NULL AND suspended=0 \
                        AND (buried_until IS NULL OR buried_until<=?2) AND due<=?2 GROUP BY phase",
                )?
                .query_map(params![deck.id, now], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (phase, count) in rows {
                counts.insert(phase, count);
            }
            let raw_new = *counts.get("new").unwrap_or(&0);
            let raw_review = *counts.get("review").unwrap_or(&0);
            deck.new_count = raw_new.min(
                deck.settings
                    .new_limit
                    .saturating_sub(self.daily_used(&deck.id, "new")?),
            );
            deck.review = raw_review.min(
                deck.settings
                    .review_limit
                    .saturating_sub(self.daily_used(&deck.id, "review")?),
            );
            deck.learning =
                counts.get("learning").unwrap_or(&0) + counts.get("relearning").unwrap_or(&0);
            deck.limited_new = raw_new - deck.new_count;
            deck.limited_review = raw_review - deck.review;
            deck.due = deck.new_count + deck.review + deck.learning;
            deck.in_review = self.conn.query_row(
                "SELECT count(*) FROM cards WHERE deck_id=?1 AND deleted_at IS NULL AND phase='review' \
                    AND suspended=0 AND (buried_until IS NULL OR buried_until<=?2)",
                params![deck.id, now],
                |r| r.get(0),
            )?;
        }
        Ok(decks)
    }
    pub fn session_record(&self) -> Result<Option<SessionRecord>> {
        Ok(self
            .metadata::<SessionRecord>("session")?
            .filter(|s| s.day == self.now().day()))
    }
    pub fn start_session(&mut self, deck_id: Option<String>) -> Result<StudyView> {
        if let Some(ref deck) = deck_id {
            self.assert_deck(deck)?;
        }
        if let Some(current) = self.study_view()?
            && current.session.deck_id == deck_id
            && !current.finished
        {
            return Ok(current);
        }
        let decks = self.decks()?;
        let clock = self.now();
        let mut cards = Vec::new();
        let mut learning = Vec::new();
        for deck in decks
            .into_iter()
            .filter(|d| deck_id.as_ref().is_none_or(|id| &d.id == id))
        {
            let base = "SELECT c.id FROM cards c JOIN notes n ON n.id=c.note_id WHERE c.deck_id=?1 AND \
                c.deleted_at IS NULL AND c.suspended=0 AND (c.buried_until IS NULL OR \
                c.buried_until<=?2) AND c.due<=?2";
            let query = |sql: String, limit: u32| -> Result<Vec<String>> {
                Ok(self
                    .conn
                    .prepare(&sql)?
                    .query_map(params![deck.id, clock.now, limit], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?)
            };
            let new = query(
                format!(
                    "{base} AND c.phase='new' ORDER BY {} LIMIT ?3",
                    if deck.settings.new_order == "random" {
                        "random()"
                    } else {
                        "n.created_at,n.rowid"
                    }
                ),
                deck.new_count,
            )?;
            let review = query(
                format!(
                    "{base} AND c.phase='review' ORDER BY {} LIMIT ?3",
                    if deck.settings.review_order == "random" {
                        "random()"
                    } else {
                        "c.due,c.id"
                    }
                ),
                deck.review,
            )?;
            learning.extend(query(
                format!("{base} AND c.phase IN ('learning','relearning') ORDER BY c.due LIMIT ?3"),
                100_000,
            )?);
            match deck.settings.new_placement.as_str() {
                "before" => {
                    cards.extend(new);
                    cards.extend(review);
                }
                "mix" => {
                    let mut n = new.into_iter();
                    let mut r = review.into_iter();
                    loop {
                        let a = r.next();
                        let b = n.next();
                        if a.is_none() && b.is_none() {
                            break;
                        }
                        cards.extend(a);
                        cards.extend(b);
                    }
                }
                _ => {
                    cards.extend(review);
                    cards.extend(new);
                }
            }
        }
        learning.extend(cards);
        let session = SessionRecord {
            id: id(),
            day: clock.day(),
            deck_id,
            card_ids: learning,
            completed: vec![],
            skipped: vec![],
        };
        self.set_metadata("session", &session)?;
        self.study_view()?
            .ok_or_else(|| AppError::invalid("The study day changed. Start a new session."))
    }
    pub fn study_view(&mut self) -> Result<Option<StudyView>> {
        let Some(mut session) = self.session_record()? else {
            return Ok(None);
        };
        let clock = self.now();
        let rows = self
            .conn
            .prepare(
                "SELECT \
                    c.id,c.due,c.deleted_at,c.suspended,c.buried_until,d.deleted_at,c.deck_id,c.phase,EXISTS(SELECT \
                    1 FROM reviews r WHERE r.card_id=c.id AND r.day=?2 AND r.undone_at IS NULL) FROM cards c \
                    JOIN decks d ON d.id=c.deck_id WHERE c.id IN (SELECT value FROM json_each(?1))",
            )?
            .query_map(params![serde_json::to_string(&session.card_ids)?, clock.day()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, bool>(8)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let states: HashMap<_, _> = rows
            .into_iter()
            .map(
                |(id, due, deleted, suspended, buried, deck_deleted, deck, phase, admitted)| {
                    (
                        id,
                        (
                            due,
                            deleted,
                            suspended,
                            buried,
                            deck_deleted,
                            deck,
                            phase,
                            admitted,
                        ),
                    )
                },
            )
            .collect();
        let mut budgets = HashMap::<(String, String), u32>::new();
        let mut current = None;
        let mut next_due = None;
        let before = session.skipped.len();
        let done: HashSet<_> = session
            .completed
            .iter()
            .chain(session.skipped.iter())
            .cloned()
            .collect();
        for card_id in &session.card_ids {
            if done.contains(card_id) {
                continue;
            }
            let Some((due, deleted, suspended, buried, deck_deleted, deck, phase, admitted)) =
                states.get(card_id)
            else {
                session.skipped.push(card_id.clone());
                continue;
            };
            if deleted.is_some()
                || *suspended
                || buried.is_some_and(|x| x > clock.now)
                || deck_deleted.is_some()
                || *due >= clock.after_days(1)
            {
                session.skipped.push(card_id.clone());
                continue;
            }
            if !admitted && (phase == "new" || phase == "review") {
                let key = (deck.clone(), phase.clone());
                if !budgets.contains_key(&key) {
                    let settings = self.deck_settings(deck)?;
                    let limit = if phase == "new" {
                        settings.new_limit
                    } else {
                        settings.review_limit
                    };
                    budgets.insert(
                        key.clone(),
                        limit.saturating_sub(self.daily_used(deck, phase)?),
                    );
                }
                let remaining = budgets.get_mut(&key).expect("budget initialized");
                if *remaining == 0 {
                    session.skipped.push(card_id.clone());
                    continue;
                }
                *remaining -= 1;
            }
            if *due <= clock.now && current.is_none() {
                current = Some(card_id.clone());
            }
            if *due > clock.now {
                next_due = Some(next_due.map_or(*due, |old: i64| old.min(*due)));
            }
        }
        if before != session.skipped.len() {
            self.set_metadata("session", &session)?;
        }
        let finished = session.completed.len() + session.skipped.len() >= session.card_ids.len();
        let card = current.as_deref().map(|id| self.card(id)).transpose()?;
        let options = if let Some(ref card) = card {
            scheduler::preview(&card.schedule, &self.deck_settings(&card.deck_id)?, clock)?
        } else {
            vec![]
        };
        Ok(Some(StudyView {
            session,
            card,
            options,
            next_due,
            finished,
        }))
    }
    /// Commits schedule, history, session progress, and undo together. Operation IDs make
    /// retries idempotent; card revisions prevent a stale view from overwriting newer work.
    pub fn review(&mut self, input: ReviewInput) -> Result<StudyView> {
        if input.operation_id.len() > 100 || input.operation_id.is_empty() {
            return Err(AppError::invalid(
                "A review requires a unique operation identifier.",
            ));
        }
        let already: Option<(String, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT card_id,undone_at FROM reviews WHERE operation_id=?1",
                [&input.operation_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((card_id, undone)) = already {
            if card_id != input.card_id || undone.is_some() {
                return Err(AppError::conflict(
                    "This review operation has already been used.",
                ));
            }
            return self.study_view()?.ok_or_else(|| {
                AppError::conflict("The review was saved, but its session has ended.")
            });
        }
        let mut session = self
            .session_record()?
            .ok_or_else(|| AppError::conflict("Start a new study session."))?;
        if session.id != input.session_id
            || !session.card_ids.contains(&input.card_id)
            || session.completed.contains(&input.card_id)
            || session.skipped.contains(&input.card_id)
        {
            return Err(AppError::conflict(
                "This card is no longer part of the active session.",
            ));
        }
        let clock = self.now();
        let card = self.card(&input.card_id)?;
        if card.revision != input.revision {
            return Err(AppError::conflict(
                "This card changed. Refresh it before grading.",
            ));
        }
        if !card.eligible(clock.now) || card.schedule.due > clock.now {
            return Err(AppError::conflict(
                "This card is not currently eligible for review.",
            ));
        }
        let settings = self.deck_settings(&card.deck_id)?;
        let category = match card.schedule.phase {
            Phase::New => "new",
            Phase::Review => "review",
            _ => "learning",
        };
        if category != "learning" {
            let admitted = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM reviews WHERE card_id=?1 AND day=?2 AND undone_at IS NULL)", params![card.id, clock.day()], |r| r.get::<_, bool>(0))?;
            let limit = if category == "new" {
                settings.new_limit
            } else {
                settings.review_limit
            };
            if !admitted && self.daily_used(&card.deck_id, category)? >= limit {
                return Err(AppError::conflict(
                    "This deck’s daily limit has been reached. Adjust the limit or return tomorrow.",
                ));
            }
        }
        let next = scheduler::next_schedule(&card.schedule, &settings, input.grade, clock)?;
        self.transaction(|store| {
            let review_id = id();
            store.save_undo(&UndoRecord {
                label: "Review".into(),
                cards: vec![card.clone()],
                session: Some(session.clone()),
                review_id: Some(review_id.clone()),
            })?;
            store.conn.execute(
                "INSERT INTO \
                    reviews(id,operation_id,card_id,deck_id,deck_name,timestamp,day,grade,category,before_state,after_state,duration_ms,scheduler,parameters) \
                    VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    review_id,
                    input.operation_id,
                    card.id,
                    card.deck_id,
                    card.deck_name,
                    clock.now,
                    clock.day(),
                    input.grade,
                    category,
                    serde_json::to_string(&card.schedule)?,
                    serde_json::to_string(&next)?,
                    input.duration_ms.min(86_400_000),
                    scheduler::SCHEDULER_VERSION,
                    serde_json::to_string(&fsrs::DEFAULT_PARAMETERS)?
                ],
            )?;
            store.write_schedule(&card.id, &next)?;
            if next.lapses >= settings.leech_threshold {
                store.conn.execute("UPDATE cards SET leech=1,suspended=CASE WHEN ?1 THEN 1 ELSE suspended END WHERE id=?2", params![settings.suspend_leeches, card.id])?;
            }
            if next.due >= clock.after_days(1) {
                session.completed.push(card.id.clone());
            }
            store.set_metadata("session", &session)?;
            store.dirty()?;
            store.study_view()?.ok_or_else(|| AppError::conflict("The study day changed during this review."))
        })
    }
    pub fn review_history(&self, card: &str, offset: u32) -> Result<Vec<ReviewEvent>> {
        Ok(self
            .conn
            .prepare(
                "SELECT \
                    id,card_id,deck_name,timestamp,day,grade,before_state,after_state,duration_ms,undone_at,scheduler \
                    FROM reviews WHERE card_id=?1 ORDER BY timestamp DESC,rowid DESC LIMIT 100 OFFSET ?2",
            )?
            .query_map(params![card, offset], |r| {
                Ok(ReviewEvent {
                    id: r.get(0)?,
                    card_id: r.get(1)?,
                    deck_name: r.get(2)?,
                    timestamp: r.get(3)?,
                    day: r.get(4)?,
                    grade: r.get(5)?,
                    before: json_column(r, 6)?,
                    after: json_column(r, 7)?,
                    duration_ms: r.get(8)?,
                    undone_at: r.get(9)?,
                    scheduler: r.get(10)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn bootstrap(&self) -> Result<Bootstrap> {
        let decks = self.decks()?;
        let clock = self.now();
        let studied = self.conn.query_row(
            "SELECT count(DISTINCT card_id) FROM reviews WHERE day=?1 AND undone_at IS NULL",
            [clock.day()],
            |r| r.get(0),
        )?;
        let next_due = self.conn.query_row(
            "SELECT min(due) FROM cards WHERE deleted_at IS NULL AND suspended=0 AND (buried_until IS \
                NULL OR buried_until<=?1) AND due>?1 AND phase!='new'",
            [clock.now],
            |r| r.get(0),
        )?;
        let today = Today {
            new_count: decks.iter().map(|d| d.new_count).sum(),
            learning: decks.iter().map(|d| d.learning).sum(),
            review: decks.iter().map(|d| d.review).sum(),
            studied,
            streak: self.streaks()?.0,
            next_due,
        };
        Ok(Bootstrap {
            decks,
            tags: self.tags()?,
            preferences: self.preferences()?,
            today,
            media_dir: self.media_dir().to_string_lossy().into(),
            data_dir: self.root.to_string_lossy().into(),
            backup_warning: self.backup_warning.clone(),
            undo_label: self.undo_record()?.map(|u| u.label),
            session: self.session_record()?,
        })
    }
}
