//! Deck relationships and the shared daily-budget allocator used by counters and study.
use crate::{
    error::{AppError, Result},
    models::*,
    store::{Store, json_column},
};
use rusqlite::{Connection, params};
use std::collections::{HashMap, HashSet};

pub(crate) const SCHEMA_VERSION: u32 = 2;

pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::invalid(
            "This collection was created by a newer Tala version. Update Tala before opening it.",
        ));
    }
    let tx = conn.transaction()?;
    if version == 0 {
        tx.execute_batch(include_str!("../migrations/001_initial.sql"))?;
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS reviews_daily_summary ON reviews(day,grade,duration_ms) WHERE undone_at IS NULL; CREATE INDEX IF NOT EXISTS reviews_first_graduation ON reviews(card_id) WHERE undone_at IS NULL AND json_extract(after_state,'$.phase')='review';")?;
    if version < 2 {
        tx.execute_batch(include_str!("../migrations/002_hierarchy.sql"))?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) struct Queue {
    pub ids: Vec<String>,
    pub counts: HashMap<String, DeckCounts>,
}

pub(crate) fn ancestors(decks: &[Deck], id: &str) -> Result<Vec<String>> {
    let by_id: HashMap<_, _> = decks.iter().map(|d| (d.id.as_str(), d)).collect();
    let mut chain = Vec::new();
    let mut current = Some(id);
    while let Some(id) = current {
        if chain.iter().any(|s| s == id) {
            return Err(AppError::invalid("The deck hierarchy contains a cycle."));
        }
        let deck = by_id
            .get(id)
            .ok_or_else(|| AppError::invalid("A parent deck is missing or deleted."))?;
        chain.push(id.to_string());
        current = deck.parent_id.as_deref();
    }
    Ok(chain)
}

pub(crate) fn hierarchy_issues(conn: &Connection) -> Result<Vec<String>> {
    let rows = conn
        .prepare("SELECT id,parent_id,deleted_at FROM decks")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let by_id: HashMap<_, _> = rows.iter().map(|r| (r.0.as_str(), r)).collect();
    let mut issues = Vec::new();
    for (id, _, deleted) in &rows {
        let mut visited = HashSet::new();
        let mut current = Some(id.as_str());
        while let Some(key) = current {
            if !visited.insert(key) {
                issues.push(format!("Deck hierarchy cycle at {id}."));
                break;
            }
            let Some(row) = by_id.get(key) else {
                issues.push(format!("Missing parent for deck {id}."));
                break;
            };
            if deleted.is_none() && row.2.is_some() {
                issues.push(format!("Deleted parent for active deck {id}."));
                break;
            }
            current = row.1.as_deref();
        }
    }
    Ok(issues)
}

impl Store {
    pub(crate) fn deck_records(&self) -> Result<Vec<Deck>> {
        let mut decks = self.conn.prepare("SELECT id,name,cover,color,settings,created_at,updated_at,parent_id FROM decks WHERE deleted_at IS NULL ORDER BY name COLLATE NOCASE,id")?.query_map([], |r| Ok(Deck {
            id: r.get(0)?, name: r.get(1)?, cover: r.get(2)?, color: r.get(3)?, settings: json_column(r,4)?, created_at:r.get(5)?, updated_at:r.get(6)?, parent_id:r.get(7)?, path:String::new(), subtree_counts:DeckCounts::default(), total:0,new_count:0,learning:0,review:0,due:0,in_review:0,limited_new:0,limited_review:0,
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let paths: Vec<_> = decks
            .iter()
            .map(|d| {
                let mut chain = ancestors(&decks, &d.id)?;
                chain.reverse();
                Ok(chain
                    .iter()
                    .map(|id| {
                        decks
                            .iter()
                            .find(|d| &d.id == id)
                            .expect("validated ancestor")
                            .name
                            .as_str()
                    })
                    .collect::<Vec<_>>()
                    .join("::"))
            })
            .collect::<Result<_>>()?;
        for (deck, path) in decks.iter_mut().zip(paths) {
            deck.path = path;
        }
        Ok(decks)
    }

    pub fn descendant_ids(&self, id: &str) -> Result<Vec<String>> {
        Ok(self.conn.prepare("WITH RECURSIVE branch(id) AS (SELECT id FROM decks WHERE id=?1 UNION SELECT d.id FROM decks d JOIN branch b ON d.parent_id=b.id WHERE d.deleted_at IS NULL) SELECT id FROM branch")?.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn ancestor_ids(&self, id: &str) -> Result<Vec<String>> {
        ancestors(&self.deck_records()?, id)
    }

    pub(crate) fn budget_remaining(&self, deck: &Deck, category: &str) -> Result<u32> {
        let limit = if category == "new" {
            deck.settings.new_limit
        } else {
            deck.settings.review_limit
        };
        Ok(limit.saturating_sub(self.daily_used(&deck.id, category)?))
    }

    pub(crate) fn allocate_queue(&self, decks: &[Deck], root: Option<&str>) -> Result<Queue> {
        // Iterative preorder avoids recursion on deeply nested imported decks.
        let mut stack: Vec<&Deck> = if let Some(id) = root {
            vec![
                decks
                    .iter()
                    .find(|d| d.id == id)
                    .ok_or_else(|| AppError::invalid("Choose an existing deck."))?,
            ]
        } else {
            decks
                .iter()
                .rev()
                .filter(|d| d.parent_id.is_none())
                .collect()
        };
        let by_id: HashMap<_, _> = decks.iter().map(|d| (d.id.as_str(), d)).collect();
        let mut visited = HashSet::new();
        let clock = self.now();
        let used: HashMap<(String, String), u32> = self.conn.prepare("SELECT s.deck_id,r.category,count(DISTINCT r.card_id) FROM reviews r JOIN review_scopes s ON s.review_id=r.id WHERE r.day=?1 AND r.undone_at IS NULL AND r.category IN ('new','review') GROUP BY s.deck_id,r.category")?.query_map([clock.day()], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut budgets = HashMap::new();
        for deck in decks {
            for category in ["new", "review"] {
                budgets.insert(
                    (deck.id.clone(), category),
                    (if category == "new" {
                        deck.settings.new_limit
                    } else {
                        deck.settings.review_limit
                    })
                    .saturating_sub(*used.get(&(deck.id.clone(), category.into())).unwrap_or(&0)),
                );
            }
        }
        let mut counts = HashMap::new();
        let mut learning = Vec::<(i64, String)>::new();
        let mut ids = Vec::new();
        while let Some(deck) = stack.pop() {
            if !visited.insert(&deck.id) {
                return Err(AppError::invalid("The deck hierarchy contains a cycle."));
            }
            stack.extend(
                decks
                    .iter()
                    .rev()
                    .filter(|d| d.parent_id.as_deref() == Some(&deck.id)),
            );
            let chain = ancestors(decks, &deck.id)?;
            let mut count = DeckCounts::default();
            let mut selected = HashMap::<&str, Vec<String>>::new();
            for category in ["new", "review"] {
                let limit = chain
                    .iter()
                    .map(|id| budgets[&(id.clone(), category)])
                    .min()
                    .unwrap_or(0);
                let order = if (category == "new" && deck.settings.new_order == "random")
                    || (category == "review" && deck.settings.review_order == "random")
                {
                    "random()"
                } else if category == "new" {
                    "n.created_at,n.rowid"
                } else {
                    "c.due,c.id"
                };
                let sql = format!(
                    "SELECT c.id FROM cards c JOIN notes n ON n.id=c.note_id WHERE c.deck_id=?1 AND c.deleted_at IS NULL AND c.suspended=0 AND (c.buried_until IS NULL OR c.buried_until<=?2) AND c.due<=?2 AND c.phase=?3 ORDER BY {order} LIMIT ?4"
                );
                let rows = self
                    .conn
                    .prepare(&sql)?
                    .query_map(params![deck.id, clock.now, category, limit], |r| {
                        r.get::<_, String>(0)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let n = rows.len() as u32;
                for id in &chain {
                    *budgets
                        .get_mut(&(id.clone(), category))
                        .expect("ancestor budget") -= n;
                }
                if category == "new" {
                    count.new_count = n;
                } else {
                    count.review = n;
                }
                selected.insert(category, rows);
            }
            let due=self.conn.prepare("SELECT due,id FROM cards WHERE deck_id=?1 AND deleted_at IS NULL AND suspended=0 AND (buried_until IS NULL OR buried_until<=?2) AND due<=?2 AND phase IN ('learning','relearning') ORDER BY due,id")?.query_map(params![deck.id,clock.now],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<rusqlite::Result<Vec<(i64,String)>>>()?;
            count.learning = due.len() as u32;
            count.due = count.new_count + count.review + count.learning;
            learning.extend(due);
            let new = selected.remove("new").unwrap_or_default();
            let review = selected.remove("review").unwrap_or_default();
            match by_id[deck.id.as_str()].settings.new_placement.as_str() {
                "before" => {
                    ids.extend(new);
                    ids.extend(review);
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
                        ids.extend(a);
                        ids.extend(b);
                    }
                }
                _ => {
                    ids.extend(review);
                    ids.extend(new);
                }
            }
            counts.insert(deck.id.clone(), count);
        }
        learning.sort();
        let mut all: Vec<_> = learning.into_iter().map(|(_, id)| id).collect();
        all.extend(ids);
        Ok(Queue { ids: all, counts })
    }
}
