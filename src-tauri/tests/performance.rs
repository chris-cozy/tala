//! Opt-in, disposable 50k-card / 500k-review acceptance benchmark.
//! Run: cargo test --release --test performance -- --ignored --nocapture
use rusqlite::params;
use std::time::Instant;
use tala_lib::{clock::Clock, content::text_document, models::*, store::Store};

#[test]
#[ignore = "large disposable collection; run explicitly for release verification"]
fn collection_scale_acceptance() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let clock = Clock::at(1_787_840_000, chrono_tz::America::New_York);
    store.clock_override = Some(clock);
    let deck = store
        .save_deck(DeckInput {
            id: None,
            name: "Performance fixture".into(),
            cover: None,
            color: "violet".into(),
            settings: DeckSettings::default(),
        })
        .unwrap();
    let new_state = Schedule::new(clock.now);
    let review_state = Schedule {
        phase: Phase::Review,
        due: clock.now - 3600,
        stability: Some(30.0),
        difficulty: Some(5.0),
        scheduled_days: 30,
        review_count: 10,
        lapses: 1,
        last_review: Some(clock.now - 30 * 86400),
        step: 0,
    };
    let answer = text_document("A searchable answer").to_string();
    let new_json = serde_json::to_string(&new_state).unwrap();
    let review_json = serde_json::to_string(&review_state).unwrap();
    let setup = Instant::now();
    store.conn.execute_batch("BEGIN").unwrap();
    {
        let mut notes = store
            .conn
            .prepare_cached("INSERT INTO notes(id,front,back,front_text,back_text,front_key,behavior,created_at,modified_at) VALUES (?1,?2,?3,?4,'A searchable answer',?4,'normal',?5,?5)")
            .unwrap();
        let mut cards = store.conn.prepare_cached("INSERT INTO cards(id,note_id,deck_id,phase,due,schedule) VALUES (?1,?1,?2,?3,?4,?5)").unwrap();
        let mut reviews = store
            .conn
            .prepare_cached("INSERT INTO reviews(id,operation_id,card_id,deck_id,deck_name,timestamp,day,grade,category,before_state,after_state,duration_ms,scheduler,parameters) VALUES (?1,?1,?2,?3,'Performance fixture',?4,?5,3,'review',?6,?7,5000,'fsrs-6/crate-6.6.1','[]')")
            .unwrap();
        for i in 0..50_000 {
            let id = format!("card-{i:05}");
            let text = format!("Biology concept {i:05}");
            notes
                .execute(params![
                    id,
                    text_document(&text).to_string(),
                    answer,
                    text,
                    clock.now - 365 * 86400 + i
                ])
                .unwrap();
            let is_new = i % 5 == 0;
            cards
                .execute(params![
                    id,
                    deck,
                    if is_new { "new" } else { "review" },
                    if is_new { clock.now } else { review_state.due },
                    if is_new { &new_json } else { &review_json }
                ])
                .unwrap();
            for j in 0..10 {
                let timestamp = clock.now - (310 - j * 30) * 86400;
                reviews
                    .execute(params![
                        format!("review-{i}-{j}"),
                        id,
                        deck,
                        timestamp,
                        clock.date_at(timestamp).to_string(),
                        if j == 0 { &new_json } else { &review_json },
                        review_json
                    ])
                    .unwrap();
            }
        }
    }
    store.conn.execute_batch("COMMIT; ANALYZE;").unwrap();
    println!(
        "Fixture: 50,000 notes/cards, 500,000 reviews; {:.2}s to generate",
        setup.elapsed().as_secs_f64()
    );
    let measure = |name: &str, task: &mut dyn FnMut(), budget_ms: f64| {
        let mut durations = Vec::new();
        for _ in 0..3 {
            let start = Instant::now();
            task();
            durations.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        println!("{name}: {durations:?} ms (budget {budget_ms} ms)");
        assert!(
            durations.iter().all(|ms| *ms < budget_ms),
            "{name} exceeded the acceptance budget"
        );
    };
    measure(
        "FTS search",
        &mut || {
            assert!(
                store
                    .browse(BrowseQuery {
                        search: "Biology concept 004".into(),
                        ..Default::default()
                    })
                    .unwrap()
                    .total
                    > 0
            );
        },
        300.0,
    );
    measure(
        "Due counts / bootstrap",
        &mut || {
            assert_eq!(store.bootstrap().unwrap().decks[0].total, 50_000);
        },
        300.0,
    );
    measure(
        "30-day statistics",
        &mut || {
            store.statistics("30").unwrap();
        },
        1000.0,
    );
    measure(
        "All-time statistics",
        &mut || {
            assert_eq!(store.statistics("all").unwrap().summary.reviewed, 500_000);
        },
        1000.0,
    );
    store.start_session(Some(deck)).unwrap();
    measure(
        "Prepared next study card",
        &mut || {
            assert!(store.study_view().unwrap().unwrap().card.is_some());
        },
        100.0,
    );
}
