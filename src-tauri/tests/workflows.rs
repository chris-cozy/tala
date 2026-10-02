use tala_lib::{clock::Clock, content::text_document, models::*, store::Store};
use tempfile::TempDir;

fn setup() -> (TempDir, Store, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    store.clock_override = Some(Clock::at(1_787_840_000, chrono_tz::America::New_York));
    let deck = store
        .save_deck(DeckInput {
            parent_id: None,
            id: None,
            name: "Biology".into(),
            cover: None,
            color: "teal".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    (dir, store, deck)
}
fn note(store: &mut Store, deck: &str, front: &str) -> CardView {
    store
        .save_note(NoteInput {
            id: None,
            deck_id: deck.into(),
            front: text_document(front),
            back: text_document("Mitochondria"),
            behavior: Behavior::Normal,
            tags: vec!["biology".into()],
        })
        .unwrap()
}
fn grade(store: &mut Store, view: &StudyView, value: u8) -> StudyView {
    let card = view.card.as_ref().unwrap();
    store
        .review(ReviewInput {
            session_id: view.session.id.clone(),
            card_id: card.id.clone(),
            revision: card.revision,
            operation_id: uuid::Uuid::new_v4().to_string(),
            grade: value,
            duration_ms: 5000,
        })
        .unwrap()
}

#[test]
fn lifecycle_persists_content_scheduling_history_and_reversible_actions() {
    let (dir, mut store, deck) = setup();
    let card = note(&mut store, &deck, "What powers a cell?");
    let session = store.start_session(Some(deck.clone())).unwrap();
    assert_eq!(session.session.card_ids.len(), 1);
    let complete = grade(&mut store, &session, 4);
    assert!(complete.finished);
    let learned = store.card(&card.id).unwrap();
    assert_eq!(learned.schedule.phase, Phase::Review);
    assert_eq!(store.statistics("30").unwrap().summary.reviewed, 1);
    assert_eq!(store.statistics("30").unwrap().summary.learned, 1);
    assert_eq!(store.streaks().unwrap().0, 1);
    store.undo().unwrap();
    assert_eq!(store.card(&card.id).unwrap().schedule.phase, Phase::New);
    assert_eq!(store.statistics("30").unwrap().summary.reviewed, 0);
    assert!(
        store.review_history(&card.id, 0).unwrap()[0]
            .undone_at
            .is_some()
    );
    let again = store.study_view().unwrap().unwrap();
    grade(&mut store, &again, 4);
    let before = store.card(&card.id).unwrap().schedule;
    store
        .save_note(NoteInput {
            id: Some(card.note_id.clone()),
            deck_id: deck.clone(),
            front: text_document("Updated question"),
            back: text_document("Updated answer"),
            behavior: Behavior::Reversed,
            tags: vec!["Chapter 1".into()],
        })
        .unwrap();
    assert_eq!(store.card(&card.id).unwrap().schedule, before);
    let second = store
        .save_deck(DeckInput {
            parent_id: None,
            id: None,
            name: "Science".into(),
            cover: None,
            color: "blue".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    store
        .bulk(BulkInput {
            ids: vec![card.id.clone()],
            action: "move".into(),
            value: Some(second),
        })
        .unwrap();
    assert_eq!(store.card(&card.id).unwrap().schedule, before);
    store.undo().unwrap();
    assert_eq!(store.card(&card.id).unwrap().deck_id, deck);
    for action in ["suspend", "bury", "delete"] {
        store
            .bulk(BulkInput {
                ids: vec![card.id.clone()],
                action: action.into(),
                value: None,
            })
            .unwrap();
        store.undo().unwrap();
        assert_eq!(store.card(&card.id).unwrap().schedule, before);
    }
    store
        .edit_tag("chapter 1", Some("foundations".into()))
        .unwrap();
    assert_eq!(store.card(&card.id).unwrap().tags, vec!["foundations"]);
    let now = store.now();
    drop(store);
    let mut reopened = Store::open(dir.path()).unwrap();
    reopened.clock_override = Some(now);
    assert_eq!(reopened.card(&card.id).unwrap().schedule, before);
    assert_eq!(reopened.review_history(&card.id, 0).unwrap().len(), 2);
    assert!(reopened.integrity().unwrap().healthy);
}

#[test]
fn learning_waits_daily_caps_bury_expiry_and_duplicate_review_submissions() {
    let (_dir, mut store, deck) = setup();
    let settings = DeckSettings {
        new_limit: 1,
        ..DeckSettings::default()
    };
    store
        .save_deck(DeckInput {
            parent_id: None,
            id: Some(deck.clone()),
            name: "Biology".into(),
            cover: None,
            color: "teal".into(),
            settings,
        })
        .unwrap();
    let card = note(&mut store, &deck, "One");
    note(&mut store, &deck, "Two");
    let session = store.start_session(Some(deck.clone())).unwrap();
    assert_eq!(session.session.card_ids.len(), 1);
    let current = session.card.as_ref().unwrap();
    let input = ReviewInput {
        session_id: session.session.id.clone(),
        card_id: current.id.clone(),
        revision: current.revision,
        operation_id: "unique-op".into(),
        grade: 1,
        duration_ms: 2300,
    };
    let waiting = store.review(input.clone()).unwrap();
    assert!(waiting.card.is_none());
    assert!(!waiting.finished);
    assert!(waiting.next_due.is_some());
    store.review(input).unwrap();
    assert_eq!(store.statistics("30").unwrap().summary.reviewed, 1);
    assert_eq!(store.decks().unwrap()[0].new_count, 0);
    let clock = store.now();
    store.clock_override = Some(Clock::at(clock.now + 60, clock.zone));
    let due = store.study_view().unwrap().unwrap();
    assert!(due.card.is_some());
    grade(&mut store, &due, 3);
    assert_eq!(store.daily_used(&deck, "new").unwrap(), 1);
    store
        .bulk(BulkInput {
            ids: vec![card.id.clone()],
            action: "bury".into(),
            value: None,
        })
        .unwrap();
    assert_eq!(
        store
            .card(&card.id)
            .unwrap()
            .effective_state(store.now().now),
        "buried"
    );
    store.clock_override = Some(Clock::at(clock.after_days(1), clock.zone));
    assert_eq!(store.decks().unwrap()[0].new_count, 1);
    assert_ne!(
        store
            .card(&card.id)
            .unwrap()
            .effective_state(store.now().now),
        "buried"
    );
    assert!(store.session_record().unwrap().is_none());
}

#[test]
fn full_native_archive_restores_media_and_history_without_losing_current_backup() {
    let (dir, mut store, deck) = setup();
    let media = store
        .attach_image(include_bytes!("../../public/tala-mark.png"))
        .unwrap();
    store
        .save_deck(DeckInput {
            parent_id: None,
            id: Some(deck.clone()),
            name: "Biology".into(),
            cover: Some(media.clone()),
            color: "teal".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    let card = store
        .save_note(NoteInput {
            id: None,
            deck_id: deck.clone(),
            front: serde_json::json!({"type":"doc","content":[{"type":"image","attrs":{"mediaId":media,"alt":"Star"}}]}),
            back: text_document("Tala"),
            behavior: Behavior::Typed,
            tags: vec![],
        })
        .unwrap();
    let session = store.start_session(None).unwrap();
    grade(&mut store, &session, 4);
    let original = store.card(&card.id).unwrap();
    let backup = store.create_backup(false).unwrap();
    note(&mut store, &deck, "After backup");
    assert_eq!(store.decks().unwrap()[0].total, 2);
    store.restore_named(&backup.name).unwrap();
    assert_eq!(store.decks().unwrap()[0].total, 1);
    assert_eq!(store.card(&card.id).unwrap().schedule, original.schedule);
    assert_eq!(store.review_history(&card.id, 0).unwrap().len(), 1);
    assert_eq!(store.decks().unwrap()[0].cover, Some(media.clone()));
    assert!(store.media_dir().join(media).exists());
    assert!(store.backups().unwrap().len() >= 2);
    assert!(store.integrity().unwrap().healthy);
    let corrupt = dir.path().join("bad.tala");
    std::fs::write(&corrupt, b"not a zip file").unwrap();
    assert!(store.restore_archive(&corrupt).is_err());
    assert_eq!(store.decks().unwrap()[0].total, 1);
}

#[test]
fn import_preview_and_validation_are_atomic_and_updates_preserve_schedule() {
    let (dir, mut store, deck) = setup();
    let card = note(&mut store, &deck, "What powers a cell?");
    let session = store.start_session(None).unwrap();
    grade(&mut store, &session, 4);
    let schedule = store.card(&card.id).unwrap().schedule;
    let path = dir.path().join("cards.csv");
    std::fs::write(&path, "Question,Answer,Tags\nWhat powers a cell?,New answer,science;cell\nOther question,Other answer,science\n").unwrap();
    let grant = store.grant(path.clone(), "import".into());
    let mut config = ImportConfig {
        path_token: grant.token,
        delimiter: "csv".into(),
        has_header: true,
        front_column: 0,
        back_column: 1,
        tags_column: Some(2),
        deck_id: deck.clone(),
        behavior: Behavior::Normal,
        duplicates: "update".into(),
        preview_digest: None,
    };
    let preview = store.preview_import(&config).unwrap();
    assert_eq!(preview.duplicates, 1);
    config.preview_digest = Some(preview.digest);
    let result = store.commit_import(config.clone()).unwrap();
    assert_eq!((result.imported, result.updated), (1, 1));
    assert_eq!(store.card(&card.id).unwrap().schedule, schedule);
    std::fs::write(&path, "Question,Answer\nValid,Answer\nInvalid,\n").unwrap();
    assert!(store.commit_import(config.clone()).is_err());
    let preview = store.preview_import(&config).unwrap();
    assert!(!preview.errors.is_empty());
    config.preview_digest = Some(preview.digest);
    assert!(store.commit_import(config).is_err());
    assert_eq!(store.decks().unwrap()[0].total, 2);
    let target = dir.path().join("export.tsv");
    let grant = store.grant(target.clone(), "delimited".into());
    assert_eq!(
        store
            .export_delimited(&grant.token, "tsv", Some(deck), vec![])
            .unwrap(),
        2
    );
    assert!(
        std::fs::read_to_string(target)
            .unwrap()
            .contains("New answer")
    );
}

#[test]
fn migration_failure_rolls_back_and_interrupted_restore_recovers_old_collection() {
    let dir = tempfile::tempdir().unwrap();
    let collection = dir.path().join("collection");
    std::fs::create_dir_all(&collection).unwrap();
    let conn = rusqlite::Connection::open(collection.join("tala.sqlite3")).unwrap();
    conn.execute_batch("CREATE TABLE notes (original TEXT); INSERT INTO notes VALUES ('keep');")
        .unwrap();
    drop(conn);
    assert!(Store::open(dir.path()).is_err());
    let conn = rusqlite::Connection::open(collection.join("tala.sqlite3")).unwrap();
    assert_eq!(
        conn.query_row("SELECT original FROM notes", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='decks'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    drop(conn);
    let (_dir, mut store, deck) = setup();
    note(&mut store, &deck, "Survives interruption");
    let root = store.root.clone();
    drop(store);
    std::fs::write(root.join("restore-journal.json"), b"{}").unwrap();
    std::fs::rename(root.join("collection"), root.join("restore-previous")).unwrap();
    std::fs::create_dir_all(root.join("collection")).unwrap();
    std::fs::write(root.join("collection/partial"), b"broken").unwrap();
    let recovered = Store::open(root).unwrap();
    assert_eq!(recovered.decks().unwrap()[0].total, 1);
}

#[test]
fn browser_filters_and_trash_restore_keep_relations() {
    let (_dir, mut store, deck) = setup();
    let card = note(&mut store, &deck, "Cellular respiration");
    assert_eq!(
        store
            .browse(BrowseQuery {
                search: "cellular resp".into(),
                tag: Some("biology".into()),
                ..Default::default()
            })
            .unwrap()
            .total,
        1
    );
    store
        .bulk(BulkInput {
            ids: vec![card.id.clone()],
            action: "delete".into(),
            value: None,
        })
        .unwrap();
    assert_eq!(store.browse(BrowseQuery::default()).unwrap().total, 0);
    assert_eq!(
        store
            .browse(BrowseQuery {
                trash: true,
                ..Default::default()
            })
            .unwrap()
            .total,
        1
    );
    store
        .bulk(BulkInput {
            ids: vec![card.id.clone()],
            action: "restore".into(),
            value: None,
        })
        .unwrap();
    assert!(store.integrity().unwrap().healthy);
}

#[test]
fn leeches_relearning_manual_scheduling_and_stale_grades_are_safe() {
    let (_dir, mut store, deck) = setup();
    let settings = DeckSettings {
        leech_threshold: 1,
        suspend_leeches: true,
        ..Default::default()
    };
    store
        .save_deck(DeckInput {
            parent_id: None,
            id: Some(deck.clone()),
            name: "Biology".into(),
            cover: None,
            color: "teal".into(),
            settings,
        })
        .unwrap();
    let card = note(&mut store, &deck, "A difficult concept");
    let session = store.start_session(None).unwrap();
    grade(&mut store, &session, 4);
    let learned = store.card(&card.id).unwrap();
    let clock = store.now();
    store.clock_override = Some(Clock::at(learned.schedule.due, clock.zone));
    let review = store.start_session(None).unwrap();
    grade(&mut store, &review, 1);
    let leech = store.card(&card.id).unwrap();
    assert!(leech.leech && leech.suspended);
    assert_eq!(leech.schedule.phase, Phase::Relearning);
    assert_eq!(leech.schedule.lapses, 1);
    store.undo().unwrap();
    let reverted = store.card(&card.id).unwrap();
    assert!(!reverted.leech && !reverted.suspended);
    assert_eq!(reverted.schedule, learned.schedule);
    let before_events = store.review_history(&card.id, 0).unwrap().len();
    for (action, value) in [
        ("set_due", Some((store.now().now + 86400).to_string())),
        ("reschedule", None),
        ("reset", None),
    ] {
        store
            .bulk(BulkInput {
                ids: vec![card.id.clone()],
                action: action.into(),
                value,
            })
            .unwrap();
        assert_eq!(
            store.review_history(&card.id, 0).unwrap().len(),
            before_events
        );
        store.undo().unwrap();
        assert_eq!(store.card(&card.id).unwrap().schedule, learned.schedule);
    }
    let stale = review.card.unwrap();
    assert!(
        store
            .review(ReviewInput {
                session_id: review.session.id,
                card_id: stale.id,
                revision: stale.revision,
                operation_id: "stale".into(),
                grade: 3,
                duration_ms: 500
            })
            .is_err()
    );
}

#[test]
fn learning_across_midnight_and_clock_rollback_do_not_grade_early() {
    let (_dir, mut store, deck) = setup();
    let now = chrono::DateTime::parse_from_rfc3339("2026-11-01T23:59:30-05:00")
        .unwrap()
        .timestamp();
    store.clock_override = Some(Clock::at(now, chrono_tz::America::New_York));
    let card = note(&mut store, &deck, "Crossing midnight");
    let session = store.start_session(None).unwrap();
    grade(&mut store, &session, 1);
    assert_eq!(store.card(&card.id).unwrap().schedule.due, now + 60);
    store.clock_override = Some(Clock::at(now + 30, chrono_tz::America::New_York));
    assert!(store.session_record().unwrap().is_none());
    assert_eq!(store.decks().unwrap()[0].learning, 0);
    store.clock_override = Some(Clock::at(now + 60, chrono_tz::America::New_York));
    assert_eq!(store.decks().unwrap()[0].learning, 1);
    let resumed = store.start_session(None).unwrap();
    assert_eq!(resumed.card.unwrap().id, card.id);
    let schedule = store.card(&card.id).unwrap().schedule;
    assert!(
        tala_lib::scheduler::next_schedule(
            &schedule,
            &DeckSettings::default(),
            3,
            Clock::at(now - 1, chrono_tz::America::New_York)
        )
        .is_err()
    );
}

#[test]
fn derived_indexes_can_be_repaired_without_changing_content_or_history() {
    let (_dir, mut store, deck) = setup();
    let card = note(&mut store, &deck, "Cell biology");
    let original = store.card(&card.id).unwrap();
    store
        .conn
        .execute(
            "UPDATE notes SET front_text='wrong index',front_key='wrong' WHERE id=?1",
            [&card.note_id],
        )
        .unwrap();
    assert!(!store.integrity().unwrap().healthy);
    let repaired = store.repair_indexes().unwrap();
    assert!(repaired.healthy);
    let after = store.card(&card.id).unwrap();
    assert_eq!(after.front, original.front);
    assert_eq!(after.schedule, original.schedule);
    assert_eq!(
        store
            .browse(BrowseQuery {
                search: "cell biology".into(),
                ..Default::default()
            })
            .unwrap()
            .total,
        1
    );
    assert_eq!(store.backups().unwrap().len(), 1);
}

#[test]
fn recovery_preserves_a_database_that_cannot_be_opened() {
    let (dir, mut store, deck) = setup();
    note(&mut store, &deck, "Recover me");
    let backup = store.create_backup(false).unwrap();
    drop(store);
    let database = dir.path().join("collection/tala.sqlite3");
    std::fs::write(&database, b"damaged original database").unwrap();
    assert!(Store::open(dir.path()).is_err());
    Store::recover_archive(dir.path(), &dir.path().join("backups").join(backup.name)).unwrap();
    let recovered = Store::open(dir.path()).unwrap();
    assert_eq!(recovered.decks().unwrap()[0].total, 1);
    let preserved = std::fs::read_dir(dir.path().join("recovery-preserved"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        std::fs::read(preserved.join("tala.sqlite3")).unwrap(),
        b"damaged original database"
    );
    assert!(recovered.integrity().unwrap().healthy);
}

#[test]
fn invalid_native_content_and_unsafe_archive_paths_never_replace_the_collection() {
    use std::io::Write;
    let (dir, mut store, deck) = setup();
    let card = note(&mut store, &deck, "Keep this card");
    let source = dir.path().join("invalid-content.tala");
    store
        .conn
        .execute(
            "UPDATE notes SET front=?1 WHERE id=?2",
            rusqlite::params![r#"{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"unsafe","marks":[{"type":"link","attrs":{"href":"javascript:alert(1)"}}]}]}]}"#, card.note_id],
        )
        .unwrap();
    store.write_archive(&source).unwrap();
    store
        .conn
        .execute(
            "UPDATE notes SET front=?1 WHERE id=?2",
            rusqlite::params![card.front.to_string(), card.note_id],
        )
        .unwrap();
    assert!(store.restore_archive(&source).is_err());
    assert_eq!(store.card(&card.id).unwrap().front, card.front);
    let unsafe_path = dir.path().join("unsafe.tala");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&unsafe_path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(br#"{"formatVersion":1,"schemaVersion":1,"createdAt":0,"files":{"tala.sqlite3":"invalid","../escaped":"invalid"}}"#).unwrap();
    zip.start_file("../escaped", options).unwrap();
    zip.write_all(b"must not escape").unwrap();
    zip.start_file("tala.sqlite3", options).unwrap();
    zip.write_all(b"invalid").unwrap();
    zip.finish().unwrap();
    assert!(store.restore_archive(&unsafe_path).is_err());
    assert!(!dir.path().join("escaped").exists());
    assert!(store.integrity().unwrap().healthy);
}

#[test]
fn automatic_backups_keep_the_configured_history_and_manual_backups() {
    let (_dir, mut store, deck) = setup();
    let mut preferences = store.preferences().unwrap();
    preferences.backup_retention = 2;
    store.save_preferences(preferences).unwrap();
    let manual = store.create_backup(false).unwrap();
    let clock = store.now();
    for day in 0..5 {
        store.clock_override = Some(Clock::at(clock.now + day * 86400, clock.zone));
        note(&mut store, &deck, &format!("Day {day}"));
        store.maybe_backup().unwrap();
        store.maybe_backup().unwrap();
    }
    let backups = store.backups().unwrap();
    assert_eq!(backups.iter().filter(|b| b.automatic).count(), 2);
    assert!(backups.iter().any(|b| b.name == manual.name));
}

#[test]
fn missing_media_is_reported_and_cleanup_never_removes_referenced_media() {
    let (_dir, mut store, deck) = setup();
    let media = store
        .attach_image(include_bytes!("../../public/tala-mark.png"))
        .unwrap();
    let card = store
        .save_note(NoteInput {
            id: None,
            deck_id: deck,
            front: serde_json::json!({"type":"doc","content":[{"type":"image","attrs":{"mediaId":media}}]}),
            back: text_document("An image"),
            behavior: Behavior::Normal,
            tags: vec![],
        })
        .unwrap();
    store
        .bulk(BulkInput {
            ids: vec![card.id],
            action: "delete".into(),
            value: None,
        })
        .unwrap();
    assert_eq!(store.cleanup_media().unwrap(), 0);
    std::fs::remove_file(store.media_dir().join(&media)).unwrap();
    let report = store.integrity().unwrap();
    assert_eq!(report.missing_media, vec![media]);
    assert!(!report.healthy);
    assert!(store.cleanup_media().is_err());
}

#[test]
fn automatic_snapshot_is_stable_during_edits_and_media_cleanup() {
    let (_dir, mut store, deck) = setup();
    let media = store
        .attach_image(include_bytes!("../../public/tala-mark.png"))
        .unwrap();
    let card = store
        .save_note(NoteInput {
            id: None,
            deck_id: deck,
            front: serde_json::json!({"type":"doc","content":[{"type":"image","attrs":{"mediaId":media}}]}),
            back: text_document("Snapshot image"),
            behavior: Behavior::Normal,
            tags: vec![],
        })
        .unwrap();
    let job = store.prepare_automatic_backup().unwrap().unwrap();
    for action in ["delete", "purge"] {
        store
            .bulk(BulkInput {
                ids: vec![card.id.clone()],
                action: action.into(),
                value: None,
            })
            .unwrap();
    }
    assert_eq!(store.cleanup_media().unwrap(), 1);
    let result = job.write();
    store.finish_automatic_backup(job, result).unwrap();
    let backup = store
        .backups()
        .unwrap()
        .into_iter()
        .find(|b| b.automatic)
        .unwrap();
    store.restore_named(&backup.name).unwrap();
    assert_eq!(store.card(&card.id).unwrap().back_text, "Snapshot image");
    assert!(store.media_dir().join(media).exists());
    assert!(store.integrity().unwrap().healthy);
}

#[test]
fn rebuilding_missing_performance_indexes_preserves_v1_data() {
    let (dir, mut store, deck) = setup();
    let card = note(&mut store, &deck, "Keep scheduling");
    store
        .conn
        .execute_batch("DROP INDEX reviews_daily_summary; DROP INDEX reviews_first_graduation;")
        .unwrap();
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(reopened.card(&card.id).unwrap().front, card.front);
    assert!(reopened.integrity().unwrap().healthy);
}

#[test]
fn study_order_is_stable_and_active_sessions_observe_changed_limits() {
    let (_directory, mut store, deck) = setup();
    let new_one = note(&mut store, &deck, "First new card");
    let new_two = note(&mut store, &deck, "Second new card");
    let review_one = note(&mut store, &deck, "Review one");
    let review_two = note(&mut store, &deck, "Review two");
    let clock = store.now();
    for (card, due) in [
        (&review_one, clock.now - 100),
        (&review_two, clock.now - 200),
    ] {
        let schedule =
            tala_lib::scheduler::next_schedule(&card.schedule, &DeckSettings::default(), 4, clock)
                .unwrap();
        let schedule = Schedule {
            due,
            last_review: Some(clock.now - 86400),
            ..schedule
        };
        store
            .conn
            .execute(
                "UPDATE cards SET phase='review',due=?1,schedule=?2 WHERE id=?3",
                rusqlite::params![due, serde_json::to_string(&schedule).unwrap(), card.id],
            )
            .unwrap();
    }
    for (placement, expected) in [
        (
            "before",
            vec![
                new_one.id.clone(),
                new_two.id.clone(),
                review_two.id.clone(),
                review_one.id.clone(),
            ],
        ),
        (
            "after",
            vec![
                review_two.id.clone(),
                review_one.id.clone(),
                new_one.id.clone(),
                new_two.id.clone(),
            ],
        ),
        (
            "mix",
            vec![
                review_two.id.clone(),
                new_one.id.clone(),
                review_one.id.clone(),
                new_two.id.clone(),
            ],
        ),
    ] {
        store
            .conn
            .execute("DELETE FROM metadata WHERE key='session'", [])
            .unwrap();
        store
            .save_deck(DeckInput {
                parent_id: None,
                id: Some(deck.clone()),
                name: "Biology".into(),
                color: "teal".into(),
                cover: None,
                settings: DeckSettings {
                    new_placement: placement.into(),
                    ..Default::default()
                },
            })
            .unwrap();
        assert_eq!(
            store
                .start_session(Some(deck.clone()))
                .unwrap()
                .session
                .card_ids,
            expected
        );
    }
    store
        .save_deck(DeckInput {
            parent_id: None,
            id: Some(deck),
            name: "Biology".into(),
            color: "teal".into(),
            cover: None,
            settings: DeckSettings {
                new_limit: 0,
                review_limit: 0,
                ..Default::default()
            },
        })
        .unwrap();
    let limited = store.study_view().unwrap().unwrap();
    assert!(limited.finished);
    assert!(limited.card.is_none());
    assert_eq!(limited.session.skipped.len(), 4);
}

#[test]
fn native_backups_reject_media_modified_after_attachment() {
    let (directory, mut store, _deck) = setup();
    let bytes = include_bytes!("../../src-tauri/icons/32x32.png");
    let media = store.attach_image(bytes).unwrap();
    std::fs::write(store.media_dir().join(media), b"not the original image").unwrap();
    let target = directory.path().join("damaged.tala");
    assert!(store.write_archive(&target).is_err());
    assert!(!target.exists());
    assert!(!store.integrity().unwrap().healthy);
}
