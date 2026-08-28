CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE decks (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, cover TEXT, color TEXT NOT NULL, settings TEXT NOT NULL,
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER
);
CREATE TABLE notes (
 id TEXT PRIMARY KEY, front TEXT NOT NULL, back TEXT NOT NULL, front_text TEXT NOT NULL, back_text TEXT NOT NULL,
 front_key TEXT NOT NULL, behavior TEXT NOT NULL CHECK(behavior IN ('normal','reversed','typed')),
 content_version INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL, modified_at INTEGER NOT NULL, deleted_at INTEGER
);
CREATE TABLE cards (
 id TEXT PRIMARY KEY, note_id TEXT NOT NULL UNIQUE REFERENCES notes(id) ON DELETE CASCADE,
 deck_id TEXT NOT NULL REFERENCES decks(id), phase TEXT NOT NULL CHECK(phase IN ('new','learning','review','relearning')),
 due INTEGER NOT NULL, schedule TEXT NOT NULL, suspended INTEGER NOT NULL DEFAULT 0 CHECK(suspended IN (0,1)),
 buried_until INTEGER, leech INTEGER NOT NULL DEFAULT 0 CHECK(leech IN (0,1)), revision INTEGER NOT NULL DEFAULT 0,
 deleted_at INTEGER
);
CREATE INDEX cards_queue ON cards(deck_id,deleted_at,suspended,phase,due);
CREATE INDEX cards_due ON cards(deleted_at,suspended,due);
CREATE INDEX notes_duplicates ON notes(front_key,behavior);
CREATE VIRTUAL TABLE note_search USING fts5(note_id UNINDEXED, front_text, back_text, tokenize='unicode61');
CREATE TRIGGER notes_search_insert AFTER INSERT ON notes BEGIN INSERT INTO note_search(note_id,front_text,back_text) VALUES (new.id,new.front_text,new.back_text); END;
CREATE TRIGGER notes_search_update AFTER UPDATE OF front_text,back_text ON notes BEGIN DELETE FROM note_search WHERE note_id=old.id; INSERT INTO note_search(note_id,front_text,back_text) VALUES (new.id,new.front_text,new.back_text); END;
CREATE TRIGGER notes_search_delete AFTER DELETE ON notes BEGIN DELETE FROM note_search WHERE note_id=old.id; END;
CREATE TABLE tags (name TEXT PRIMARY KEY);
CREATE TABLE note_tags (note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE, tag TEXT NOT NULL REFERENCES tags(name) ON UPDATE CASCADE ON DELETE CASCADE, PRIMARY KEY(note_id,tag));
CREATE INDEX tags_notes ON note_tags(tag,note_id);
CREATE TABLE media (id TEXT PRIMARY KEY, mime TEXT NOT NULL, bytes INTEGER NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE note_media (note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE, media_id TEXT NOT NULL REFERENCES media(id), PRIMARY KEY(note_id,media_id));
CREATE TABLE reviews (
 id TEXT PRIMARY KEY, operation_id TEXT NOT NULL UNIQUE, card_id TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
 deck_id TEXT NOT NULL, deck_name TEXT NOT NULL, timestamp INTEGER NOT NULL, day TEXT NOT NULL,
 grade INTEGER NOT NULL CHECK(grade BETWEEN 1 AND 4), category TEXT NOT NULL,
 before_state TEXT NOT NULL, after_state TEXT NOT NULL, duration_ms INTEGER NOT NULL,
 scheduler TEXT NOT NULL, parameters TEXT NOT NULL, undone_at INTEGER
);
CREATE INDEX reviews_card ON reviews(card_id,timestamp DESC);
CREATE INDEX reviews_day ON reviews(day,undone_at);
CREATE INDEX reviews_budget ON reviews(deck_id,day,category,undone_at,card_id);
CREATE INDEX reviews_daily_summary ON reviews(day,grade,duration_ms) WHERE undone_at IS NULL;
CREATE INDEX reviews_first_graduation ON reviews(card_id) WHERE undone_at IS NULL AND json_extract(after_state,'$.phase')='review';
CREATE TABLE review_reversals (id TEXT PRIMARY KEY, review_id TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE, timestamp INTEGER NOT NULL);
CREATE TABLE undo (singleton INTEGER PRIMARY KEY CHECK(singleton=1), data TEXT NOT NULL);
PRAGMA user_version=1;
