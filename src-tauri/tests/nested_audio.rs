use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write};
use tala_lib::{clock::Clock, content::text_document, models::*, store::Store};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

fn setup() -> (TempDir, Store, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    store.clock_override = Some(Clock::at(1_787_840_000, chrono_tz::America::New_York));
    let deck = store
        .save_deck(DeckInput {
            id: None,
            parent_id: None,
            name: "Root".into(),
            cover: None,
            color: "teal".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    (dir, store, deck)
}

fn child(store: &mut Store, parent_id: &str, name: &str, settings: DeckSettings) -> String {
    store
        .save_deck(DeckInput {
            id: None,
            parent_id: Some(parent_id.into()),
            name: name.into(),
            cover: None,
            color: "blue".into(),
            settings,
        })
        .unwrap()
}

fn note(store: &mut Store, deck_id: &str, text: &str) -> CardView {
    store
        .save_note(NoteInput {
            id: None,
            deck_id: deck_id.into(),
            front: text_document(text),
            back: text_document("Answer"),
            behavior: Behavior::Normal,
            tags: vec![],
        })
        .unwrap()
}

fn grade(store: &mut Store, view: &StudyView) {
    let card = view.card.as_ref().unwrap();
    store
        .review(ReviewInput {
            session_id: view.session.id.clone(),
            card_id: card.id.clone(),
            revision: card.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: 4,
            duration_ms: 1000,
        })
        .unwrap();
}

fn browse(store: &Store, deck: &str, only_this_deck: bool) -> Vec<CardView> {
    store
        .browse(BrowseQuery {
            deck: Some(deck.into()),
            only_this_deck,
            ..BrowseQuery::default()
        })
        .unwrap()
        .cards
}

#[test]
fn hierarchy_creation_moves_and_cycles_are_validated() {
    let (_dir, mut store, root) = setup();
    let first = child(&mut store, &root, "First", DeckSettings::default());
    let second = child(&mut store, &root, "Second", DeckSettings::default());
    let leaf = child(&mut store, &first, "Leaf", DeckSettings::default());

    let invalid_cycle = store.save_deck(DeckInput {
        id: Some(root.clone()),
        parent_id: Some(leaf.clone()),
        name: "Root".into(),
        cover: None,
        color: "teal".into(),
        settings: DeckSettings::default(),
    });
    assert!(invalid_cycle.is_err());

    store
        .save_deck(DeckInput {
            id: Some(leaf.clone()),
            parent_id: Some(second.clone()),
            name: "Leaf".into(),
            cover: None,
            color: "blue".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    let decks = store.decks().unwrap();
    assert_eq!(
        decks
            .iter()
            .find(|d| d.id == leaf)
            .unwrap()
            .parent_id
            .as_deref(),
        Some(second.as_str())
    );
    assert!(store.descendant_ids(&root).unwrap().contains(&first));
}

#[test]
fn parent_caps_apply_across_siblings_and_child_only_sessions() {
    let (_dir, mut store, root) = setup();
    store
        .save_deck(DeckInput {
            id: Some(root.clone()),
            parent_id: None,
            name: "Root".into(),
            cover: None,
            color: "teal".into(),
            settings: DeckSettings {
                new_limit: 1,
                ..DeckSettings::default()
            },
        })
        .unwrap();
    let left = child(&mut store, &root, "Left", DeckSettings::default());
    let right = child(&mut store, &root, "Right", DeckSettings::default());
    let left_card = note(&mut store, &left, "Left card");
    note(&mut store, &right, "Right card");

    let parent_session = store.start_session(Some(root.clone())).unwrap();
    assert_eq!(parent_session.session.card_ids.len(), 1);
    assert_eq!(
        store
            .decks()
            .unwrap()
            .iter()
            .find(|d| d.id == root)
            .unwrap()
            .subtree_counts
            .total,
        2
    );

    let child_session = store.start_session(Some(left.clone())).unwrap();
    assert_eq!(child_session.session.card_ids, vec![left_card.id.clone()]);
    grade(&mut store, &child_session);
    let other_child_session = store.start_session(Some(right)).unwrap();
    assert!(other_child_session.session.card_ids.is_empty());
}

#[test]
fn ancestor_review_cap_blocks_sibling_session_and_undo_releases_budget() {
    let (_dir, mut store, root) = setup();
    let left = child(&mut store, &root, "Left", DeckSettings::default());
    let right = child(&mut store, &root, "Right", DeckSettings::default());
    let left_card = note(&mut store, &left, "Left review");
    let right_card = note(&mut store, &right, "Right review");

    // Graduate each card separately so the root's review cap doesn't affect setup.
    for deck in [&left, &right] {
        let session = store.start_session(Some(deck.clone())).unwrap();
        let current = session.card.as_ref().unwrap();
        store
            .review(ReviewInput {
                session_id: session.session.id.clone(),
                card_id: current.id.clone(),
                revision: current.revision,
                operation_id: uuid::Uuid::new_v4().to_string(),
                grade: 4,
                duration_ms: 500,
            })
            .unwrap();
    }

    let left_due = store.card(&left_card.id).unwrap().schedule.due;
    let right_due = store.card(&right_card.id).unwrap().schedule.due;
    store.clock_override = Some(Clock::at(
        left_due.max(right_due),
        chrono_tz::America::New_York,
    ));
    store
        .save_deck(DeckInput {
            id: Some(root.clone()),
            parent_id: None,
            name: "Root".into(),
            cover: None,
            color: "teal".into(),
            settings: DeckSettings {
                review_limit: 1,
                ..DeckSettings::default()
            },
        })
        .unwrap();

    let left_session = store.start_session(Some(left)).unwrap();
    assert_eq!(left_session.session.card_ids, vec![left_card.id]);
    let current = left_session.card.as_ref().unwrap();
    store
        .review(ReviewInput {
            session_id: left_session.session.id,
            card_id: current.id.clone(),
            revision: current.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: 4,
            duration_ms: 500,
        })
        .unwrap();

    let blocked = store.start_session(Some(right.clone())).unwrap();
    assert!(blocked.session.card_ids.is_empty());

    store.undo().unwrap();
    let available = store.start_session(Some(right)).unwrap();
    assert_eq!(available.session.card_ids, vec![right_card.id]);
}

#[test]
fn learning_repetitions_resume_after_midnight_with_elapsed_delays() {
    let (_dir, mut store, deck) = setup();
    let before_midnight = chrono::DateTime::parse_from_rfc3339("2026-09-30T23:55:00-04:00")
        .unwrap()
        .timestamp();
    store.clock_override = Some(Clock::at(before_midnight, chrono_tz::America::New_York));
    store
        .save_deck(DeckInput {
            id: Some(deck.clone()),
            parent_id: None,
            name: "Root".into(),
            cover: None,
            color: "teal".into(),
            settings: DeckSettings {
                learning_steps: vec![60, 600],
                ..DeckSettings::default()
            },
        })
        .unwrap();
    let card = note(&mut store, &deck, "Cross midnight");
    let session = store.start_session(Some(deck.clone())).unwrap();
    let current = session.card.as_ref().unwrap();
    let repeated = store
        .review(ReviewInput {
            session_id: session.session.id.clone(),
            card_id: current.id.clone(),
            revision: current.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: 3,
            duration_ms: 1000,
        })
        .unwrap();
    let scheduled = store.card(&card.id).unwrap();
    assert_eq!(scheduled.schedule.phase, Phase::Learning);
    assert_eq!(scheduled.schedule.step, 1);
    assert_eq!(scheduled.schedule.due, before_midnight + 600);
    assert_ne!(
        Clock::at(scheduled.schedule.due, chrono_tz::America::New_York).day(),
        session.session.day
    );
    assert!(repeated.card.is_none());

    store.clock_override = Some(Clock::at(
        scheduled.schedule.due,
        chrono_tz::America::New_York,
    ));
    assert!(store.session_record().unwrap().is_none());
    let resumed = store.start_session(Some(deck)).unwrap();
    assert_eq!(resumed.card.as_ref().unwrap().id, card.id);
    let current = resumed.card.as_ref().unwrap();
    let completed = store
        .review(ReviewInput {
            session_id: resumed.session.id,
            card_id: current.id.clone(),
            revision: current.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: 3,
            duration_ms: 1000,
        })
        .unwrap();
    assert!(completed.finished);
    assert_eq!(store.card(&card.id).unwrap().schedule.phase, Phase::Review);
}

#[test]
fn resetting_relearning_card_to_new_obeys_new_card_admission_cap() {
    let (_dir, mut store, deck) = setup();
    let card = note(&mut store, &deck, "Reset during relearning");

    // Graduate the new card into Review.
    let first = store.start_session(Some(deck.clone())).unwrap();
    let first_card = first.card.as_ref().unwrap();
    store
        .review(ReviewInput {
            session_id: first.session.id.clone(),
            card_id: first_card.id.clone(),
            revision: first_card.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: 4,
            duration_ms: 500,
        })
        .unwrap();
    let learning = store.card(&card.id).unwrap();
    store.clock_override = Some(Clock::at(
        learning.schedule.due,
        chrono_tz::America::New_York,
    ));
    let second = store.start_session(Some(deck.clone())).unwrap();
    let second_card = second.card.as_ref().unwrap();
    store
        .review(ReviewInput {
            session_id: second.session.id.clone(),
            card_id: second_card.id.clone(),
            revision: second_card.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: 4,
            duration_ms: 500,
        })
        .unwrap();

    // A failed review begins Relearning, then a reset changes the card category to New.
    let review = store.card(&card.id).unwrap();
    store.clock_override = Some(Clock::at(review.schedule.due, chrono_tz::America::New_York));
    let third = store.start_session(Some(deck.clone())).unwrap();
    let third_card = third.card.as_ref().unwrap();
    store
        .review(ReviewInput {
            session_id: third.session.id.clone(),
            card_id: third_card.id.clone(),
            revision: third_card.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: 1,
            duration_ms: 500,
        })
        .unwrap();
    assert_eq!(
        store.card(&card.id).unwrap().schedule.phase,
        Phase::Relearning
    );
    store
        .bulk(BulkInput {
            ids: vec![card.id.clone()],
            action: "reset".into(),
            value: None,
        })
        .unwrap();
    assert_eq!(store.card(&card.id).unwrap().schedule.phase, Phase::New);
    store
        .save_deck(DeckInput {
            id: Some(deck.clone()),
            parent_id: None,
            name: "Root".into(),
            cover: None,
            color: "teal".into(),
            settings: DeckSettings {
                new_limit: 0,
                ..DeckSettings::default()
            },
        })
        .unwrap();

    assert!(
        store
            .start_session(Some(deck))
            .unwrap()
            .session
            .card_ids
            .is_empty()
    );
}

#[test]
fn active_session_rejects_cards_moved_out_of_its_branch() {
    let (_dir, mut store, root) = setup();
    let branch = child(&mut store, &root, "Branch", DeckSettings::default());
    let outside = store
        .save_deck(DeckInput {
            id: None,
            parent_id: None,
            name: "Outside".into(),
            cover: None,
            color: "rose".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    let card = note(&mut store, &branch, "Moves out of active branch");
    let session = store.start_session(Some(root.clone())).unwrap();
    store
        .save_note(NoteInput {
            id: Some(card.note_id.clone()),
            deck_id: outside.clone(),
            front: text_document("Moved card"),
            back: text_document("Answer"),
            behavior: Behavior::Normal,
            tags: vec![],
        })
        .unwrap();
    assert!(store.study_view().unwrap().unwrap().finished);
    assert!(
        store
            .review(ReviewInput {
                session_id: session.session.id,
                card_id: card.id.clone(),
                revision: card.revision,
                operation_id: uuid::Uuid::new_v4().to_string(),
                grade: 4,
                duration_ms: 1000,
            })
            .is_err()
    );
    assert_eq!(store.card(&card.id).unwrap().deck_id, outside);
}

#[test]
fn review_scope_tracks_reparented_branch_and_undo_removes_attribution() {
    let (_dir, mut store, old_root) = setup();
    let new_root = store
        .save_deck(DeckInput {
            id: None,
            parent_id: None,
            name: "Other".into(),
            cover: None,
            color: "rose".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    let branch = child(&mut store, &old_root, "Branch", DeckSettings::default());
    let card = note(&mut store, &branch, "Reparented card");
    store
        .save_deck(DeckInput {
            id: Some(branch.clone()),
            parent_id: Some(new_root.clone()),
            name: "Branch".into(),
            cover: None,
            color: "blue".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    let session = store.start_session(Some(new_root.clone())).unwrap();
    grade(&mut store, &session);
    assert_eq!(store.daily_used(&new_root, "new").unwrap(), 1);
    assert_eq!(store.daily_used(&old_root, "new").unwrap(), 0);
    store.undo().unwrap();
    assert_eq!(store.daily_used(&new_root, "new").unwrap(), 0);
    assert_eq!(store.card(&card.id).unwrap().deck_id, branch);
}

#[test]
fn browsing_can_limit_to_direct_deck_or_include_descendants() {
    let (_dir, mut store, root) = setup();
    let nested = child(&mut store, &root, "Nested", DeckSettings::default());
    let direct = note(&mut store, &root, "Root card");
    let nested_card = note(&mut store, &nested, "Nested card");
    assert_eq!(
        browse(&store, &root, true)
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>(),
        vec![direct.id.clone()]
    );
    let mut subtree_ids = browse(&store, &root, false)
        .iter()
        .map(|c| c.id.clone())
        .collect::<Vec<_>>();
    subtree_ids.sort();
    let mut expected = vec![direct.id, nested_card.id];
    expected.sort();
    assert_eq!(subtree_ids, expected);
}

#[test]
fn deleting_parent_promotes_children_and_keeps_their_cards() {
    let (_dir, mut store, root) = setup();
    let nested = child(&mut store, &root, "Nested", DeckSettings::default());
    let card = note(&mut store, &nested, "Survives parent deletion");
    store.delete_deck_scope(&root, None, false).unwrap();
    let decks = store.decks().unwrap();
    let promoted = decks.iter().find(|d| d.id == nested).unwrap();
    assert!(promoted.parent_id.is_none());
    assert_eq!(store.card(&card.id).unwrap().deck_id, nested);
    assert!(store.integrity().unwrap().healthy);
}

fn tiny_wav() -> Vec<u8> {
    let samples: [i16; 4] = [0, 100, -100, 0];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36u32 + (samples.len() * 2) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&8000u32.to_le_bytes());
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn audio_doc(media_id: &str) -> serde_json::Value {
    serde_json::json!({"type":"doc","content":[{"type":"audio","attrs":{"mediaId":media_id,"label":""}}]})
}

#[test]
fn wav_audio_only_cards_save_and_media_type_mismatches_are_rejected() {
    let (_dir, mut store, deck) = setup();
    let wav = tiny_wav();
    let audio_id = store.attach_audio(&wav).unwrap();
    let card = store
        .save_note(NoteInput {
            id: None,
            deck_id: deck.clone(),
            front: audio_doc(&audio_id),
            back: text_document("Audio answer"),
            behavior: Behavior::Normal,
            tags: vec![],
        })
        .unwrap();
    assert_eq!(card.front["content"][0]["type"], "audio");
    assert!(store.attach_image(&wav).is_err());
    assert!(store.attach_audio(b"not a wav").is_err());
    let image_id = store
        .attach_image(include_bytes!("../../public/tala-mark.png"))
        .unwrap();
    let wrong_type = store.save_note(NoteInput {
        id: Some(card.note_id),
        deck_id: deck,
        front: audio_doc(&image_id),
        back: text_document("Audio answer"),
        behavior: Behavior::Normal,
        tags: vec![],
    });
    assert!(wrong_type.is_err());
}

#[test]
fn schema_v1_archive_migrates_and_audio_backup_roundtrips() {
    let (dir, mut store, deck) = setup();
    let wav = tiny_wav();
    let audio_id = store.attach_audio(&wav).unwrap();
    let card = store
        .save_note(NoteInput {
            id: None,
            deck_id: deck.clone(),
            front: audio_doc(&audio_id),
            back: text_document("Backup answer"),
            behavior: Behavior::Normal,
            tags: vec![],
        })
        .unwrap();
    let archive = dir.path().join("audio.tala");
    store.write_archive(&archive).unwrap();
    store
        .save_note(NoteInput {
            id: Some(card.note_id.clone()),
            deck_id: deck,
            front: text_document("Changed after backup"),
            back: text_document("Changed"),
            behavior: Behavior::Normal,
            tags: vec![],
        })
        .unwrap();
    store.restore_archive(&archive).unwrap();
    assert_eq!(store.card(&card.id).unwrap().front, audio_doc(&audio_id));
    assert!(store.media_dir().join(&audio_id).is_file());
    assert!(store.integrity().unwrap().healthy);

    // Construct a valid version-1 archive using the checked-in initial schema.
    let legacy_dir = tempfile::tempdir().unwrap();
    let db_path = legacy_dir.path().join("tala.sqlite3");
    let legacy = Connection::open(&db_path).unwrap();
    legacy
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    legacy
        .execute_batch("DROP INDEX reviews_daily_summary; DROP INDEX reviews_first_graduation;")
        .unwrap();
    drop(legacy);
    let db_bytes = fs::read(&db_path).unwrap();
    let mut files = BTreeMap::new();
    files.insert("tala.sqlite3", hex::encode(Sha256::digest(&db_bytes)));
    let manifest =
        serde_json::json!({"formatVersion":1,"schemaVersion":1,"createdAt":0,"files":files});
    let legacy_archive = legacy_dir.path().join("legacy.tala");
    let mut zip = ZipWriter::new(fs::File::create(&legacy_archive).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
        .unwrap();
    zip.start_file("tala.sqlite3", options).unwrap();
    zip.write_all(&db_bytes).unwrap();
    zip.finish().unwrap();
    store.restore_archive(&legacy_archive).unwrap();
    assert_eq!(
        store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        2
    );
    assert!(store.integrity().unwrap().healthy);
}
