ALTER TABLE decks ADD COLUMN parent_id TEXT REFERENCES decks(id);
CREATE INDEX decks_parent ON decks(parent_id,deleted_at);
CREATE TABLE review_scopes (
 review_id TEXT NOT NULL REFERENCES reviews(id) ON DELETE CASCADE,
 deck_id TEXT NOT NULL REFERENCES decks(id),
 PRIMARY KEY(review_id,deck_id)
);
CREATE INDEX review_scopes_deck ON review_scopes(deck_id,review_id);
INSERT INTO review_scopes(review_id,deck_id)
 SELECT r.id,r.deck_id FROM reviews r JOIN decks d ON d.id=r.deck_id;
CREATE TABLE anki_sources (
 guid TEXT NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal>=0),
 note_id TEXT NOT NULL UNIQUE REFERENCES notes(id) ON DELETE CASCADE,
 PRIMARY KEY(guid,ordinal)
);
PRAGMA user_version=2;
