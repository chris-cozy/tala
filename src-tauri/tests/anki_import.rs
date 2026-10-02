use prost::Message;
use rusqlite::{Connection, params};
use sha1::{Digest as Sha1Digest, Sha1};
use std::{collections::BTreeMap, fs, io::Write, path::Path};
use tala_lib::{models::*, store::Store};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[derive(Clone, PartialEq, Message)]
struct Meta {
    #[prost(int32, tag = "1")]
    version: i32,
}
#[derive(Clone, PartialEq, Message)]
struct MediaEntries {
    #[prost(message, repeated, tag = "1")]
    entries: Vec<MediaEntry>,
}
#[derive(Clone, PartialEq, Message)]
struct MediaEntry {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(uint32, tag = "2")]
    size: u32,
    #[prost(bytes = "vec", tag = "3")]
    sha1: Vec<u8>,
    #[prost(uint32, optional, tag = "255")]
    legacy_zip_filename: Option<u32>,
}
#[derive(Clone, PartialEq, Message)]
struct NotetypeConfig {
    #[prost(uint32, tag = "1")]
    kind: u32,
    #[prost(string, tag = "3")]
    css: String,
}
#[derive(Clone, PartialEq, Message)]
struct TemplateConfig {
    #[prost(string, tag = "1")]
    question: String,
    #[prost(string, tag = "2")]
    answer: String,
}

fn wav() -> Vec<u8> {
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

fn legacy_models() -> String {
    serde_json::json!({"1": {
        "type": 0,
        "flds": [{"name":"Front","ord":0},{"name":"Back","ord":1},{"name":"Extra","ord":2}],
        "tmpls": [
            {"ord":0,"qfmt":"{{#Extra}}<b>{{Front}}</b>{{/Extra}}<script>alert(1)</script>","afmt":"{{FrontSide}}<hr>{{Back}} [sound:sound.wav]"},
            {"ord":1,"qfmt":"{{Front}}<br>{{type:Back}}","afmt":"{{FrontSide}}<hr>{{Back}}"},
            {"ord":2,"qfmt":"{{Back}}","afmt":"{{FrontSide}}<hr>{{Front}}"}
        ]
    }}).to_string()
}

fn create_legacy(path: &Path, cards: &[(i64, &str, u32)], include_audio: bool) {
    let db_path = path.with_extension("sqlite3");
    let _ = fs::remove_file(&db_path);
    let db = Connection::open(&db_path).unwrap();
    db.execute_batch("CREATE TABLE col(id INTEGER,models TEXT,decks TEXT); CREATE TABLE notes(id INTEGER PRIMARY KEY,guid TEXT,mid INTEGER,flds TEXT,tags TEXT); CREATE TABLE cards(id INTEGER PRIMARY KEY,nid INTEGER,ord INTEGER, did INTEGER, odid INTEGER);").unwrap();
    let models = legacy_models();
    let decks = serde_json::json!({"1":{"name":"Language::Basics"},"2":{"name":"Language::Verbs"}})
        .to_string();
    db.execute("INSERT INTO col VALUES(1,?1,?2)", params![models, decks])
        .unwrap();
    for (index, (ordinal, front, deck_id)) in cards.iter().enumerate() {
        let nid = index as i64 + 10;
        db.execute(
            "INSERT INTO notes VALUES(?1,?2,1,?3,'tag-one tag-two')",
            params![
                nid,
                format!("guid-{nid}"),
                format!("{front}\u{1f}Back {nid}\u{1f}yes")
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO cards VALUES(?1,?2,?3,?4,0)",
            params![index as i64 + 20, nid, ordinal, deck_id],
        )
        .unwrap();
    }
    drop(db);
    let sqlite = fs::read(&db_path).unwrap();
    let target = fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(target);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("collection.anki2", options).unwrap();
    zip.write_all(&sqlite).unwrap();
    let mut mapping = BTreeMap::new();
    mapping.insert("0".to_string(), "sound.wav".to_string());
    zip.start_file("media", options).unwrap();
    zip.write_all(serde_json::to_string(&mapping).unwrap().as_bytes())
        .unwrap();
    if include_audio {
        zip.start_file("0", options).unwrap();
        zip.write_all(&wav()).unwrap();
    }
    zip.finish().unwrap();
}

fn modern(path: &Path) {
    let db_path = path.with_extension("sqlite3");
    let db = Connection::open(&db_path).unwrap();
    db.execute_batch("CREATE TABLE decks(id INTEGER PRIMARY KEY,name TEXT); CREATE TABLE notetypes(id INTEGER PRIMARY KEY,config BLOB); CREATE TABLE fields(ntid INTEGER,ord INTEGER,name TEXT); CREATE TABLE templates(ntid INTEGER,ord INTEGER,config BLOB); CREATE TABLE notes(id INTEGER PRIMARY KEY,guid TEXT,mid INTEGER,flds TEXT,tags TEXT); CREATE TABLE cards(id INTEGER PRIMARY KEY,nid INTEGER,ord INTEGER,did INTEGER,odid INTEGER);").unwrap();
    db.execute("INSERT INTO decks VALUES(1,'Modern::Audio')", [])
        .unwrap();
    db.execute(
        "INSERT INTO notetypes VALUES(1,?1)",
        [NotetypeConfig {
            kind: 0,
            css: String::new(),
        }
        .encode_to_vec()],
    )
    .unwrap();
    db.execute("INSERT INTO fields VALUES(1,0,'Front'),(1,1,'Back')", [])
        .unwrap();
    db.execute(
        "INSERT INTO templates VALUES(1,0,?1)",
        [TemplateConfig {
            question: "{{Front}}".into(),
            answer: "{{Back}}".into(),
        }
        .encode_to_vec()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO notes VALUES(10,'modern-guid',1,'Modern front\u{1f}[sound:beep.wav]','')",
        [],
    )
    .unwrap();
    db.execute("INSERT INTO cards VALUES(20,10,0,1,0)", [])
        .unwrap();
    drop(db);
    let sqlite = fs::read(&db_path).unwrap();
    let audio = wav();
    let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("meta", options).unwrap();
    zip.write_all(&Meta { version: 3 }.encode_to_vec()).unwrap();
    zip.start_file("collection.anki21b", options).unwrap();
    let compressed_db = zstd::stream::encode_all(sqlite.as_slice(), 0).unwrap();
    zip.write_all(&compressed_db).unwrap();
    let entry = MediaEntry {
        name: "beep.wav".into(),
        size: audio.len() as u32,
        sha1: Sha1::digest(&audio).to_vec(),
        legacy_zip_filename: None,
    };
    zip.start_file("media", options).unwrap();
    zip.write_all(
        &zstd::stream::encode_all(
            MediaEntries {
                entries: vec![entry],
            }
            .encode_to_vec()
            .as_slice(),
            0,
        )
        .unwrap(),
    )
    .unwrap();
    zip.start_file("0", options).unwrap();
    zip.write_all(&zstd::stream::encode_all(audio.as_slice(), 0).unwrap())
        .unwrap();
    zip.finish().unwrap();
}

fn setup() -> (TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

fn config(store: &mut Store, path: &Path, duplicates: &str) -> AnkiImportConfig {
    let grant = store.grant(path.to_path_buf(), "import".into());
    AnkiImportConfig {
        path_token: grant.token,
        parent_id: None,
        duplicates: duplicates.into(),
        skip_affected: false,
        destinations: BTreeMap::new(),
        preview_digest: None,
    }
}

#[test]
fn legacy_package_converts_basic_typed_conditional_reverse_and_reports_scripts() {
    let (dir, mut store) = setup();
    let package = dir.path().join("source.anki2");
    create_legacy(
        &package,
        &[
            (0, "<em>Question</em>", 1),
            (1, "Typed question", 1),
            (2, "Original front", 2),
        ],
        true,
    );
    let config = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&config).unwrap();
    assert_eq!(preview.total, 3);
    assert_eq!(preview.audio_files, 1);
    assert!(!preview.warnings.is_empty());
    assert!(preview.cards[0].front.to_string().contains("Question"));
    assert_eq!(
        preview.cards[1].behavior,
        Behavior::Typed,
        "preview cards: {:?}",
        preview.cards
    );
    assert!(preview.cards[1].back.to_string().contains("Back 11"));
    assert!(preview.cards[2].front.to_string().contains("Back 12"));
    assert!(preview.cards[2].back.to_string().contains("Original front"));
    assert_eq!(preview.decks.len(), 3);
}

fn assert_legacy_heading(
    doc: &serde_json::Value,
    prefix: &str,
    title: &str,
    level: u64,
    suffix: Option<&str>,
) {
    let blocks = doc["content"].as_array().unwrap();
    assert_eq!(blocks[0]["type"], "paragraph");
    assert_eq!(tala_lib::content::plain_text(&blocks[0]), prefix);
    assert_eq!(blocks[1]["type"], "heading");
    assert_eq!(blocks[1]["attrs"]["level"], level);
    assert_eq!(tala_lib::content::plain_text(&blocks[1]), title);
    if let Some(suffix) = suffix {
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[2]["type"], "paragraph");
        assert_eq!(tala_lib::content::plain_text(&blocks[2]), suffix);
    } else {
        assert_eq!(blocks.len(), 2);
    }
}

#[test]
fn legacy_heading_does_not_capture_preceding_or_following_plain_text() {
    let (dir, mut store) = setup();
    let package = dir.path().join("headings.anki2");
    create_legacy(
        &package,
        &[
            (0, "Plain prefix<h2>Heading</h2>", 1),
            (0, "Before<h3>Third</h3>After", 1),
        ],
        true,
    );

    let mut import = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&import).unwrap();
    assert_eq!(preview.cards.len(), 2);
    assert_legacy_heading(&preview.cards[0].front, "Plain prefix", "Heading", 2, None);
    assert_legacy_heading(&preview.cards[1].front, "Before", "Third", 3, Some("After"));

    import.preview_digest = Some(preview.digest);
    assert_eq!(store.commit_anki(import).unwrap().imported, 2);
    let committed = store.browse(BrowseQuery::default()).unwrap().cards;
    assert_eq!(committed.len(), 2);
    assert_legacy_heading(&committed[0].front, "Plain prefix", "Heading", 2, None);
    assert_legacy_heading(&committed[1].front, "Before", "Third", 3, Some("After"));
}

#[test]
fn duplicate_skip_update_separate_reimport_and_stale_digest_are_handled() {
    let (dir, mut store) = setup();
    let package = dir.path().join("duplicates.apkg");
    create_legacy(&package, &[(0, "Original", 1)], true);
    let mut initial = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&initial).unwrap();
    initial.preview_digest = Some(preview.digest);
    assert_eq!(store.commit_anki(initial).unwrap().imported, 1);
    let imported = store
        .browse(BrowseQuery::default())
        .unwrap()
        .cards
        .remove(0);

    let mut skip = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&skip).unwrap();
    assert_eq!(preview.duplicates, 1);
    skip.preview_digest = Some(preview.digest);
    assert_eq!(store.commit_anki(skip).unwrap().skipped, 1);

    create_legacy(&package, &[(0, "Updated source", 1)], true);
    let mut stale = config(&mut store, &package, "update");
    let preview = store.preview_anki(&stale).unwrap();
    stale.preview_digest = Some(preview.digest);
    create_legacy(&package, &[(0, "Changed after preview", 1)], true);
    assert!(store.commit_anki(stale).is_err());

    let mut update = config(&mut store, &package, "update");
    let preview = store.preview_anki(&update).unwrap();
    update.preview_digest = Some(preview.digest);
    assert_eq!(store.commit_anki(update).unwrap().updated, 1);
    let after_update = store.card(&imported.id).unwrap();
    assert!(
        after_update
            .front
            .to_string()
            .contains("Changed after preview")
    );

    let mut separate = config(&mut store, &package, "separate");
    let preview = store.preview_anki(&separate).unwrap();
    separate.preview_digest = Some(preview.digest);
    assert_eq!(store.commit_anki(separate).unwrap().imported, 1);
    assert_eq!(store.browse(BrowseQuery::default()).unwrap().total, 2);
}

#[test]
fn missing_media_requires_explicit_skip_and_corrupt_archives_are_rejected() {
    let (dir, mut store) = setup();
    let package = dir.path().join("missing.apkg");
    create_legacy(
        &package,
        &[(0, "With audio", 1), (2, "Supported card", 1)],
        false,
    );
    let mut import_config = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&import_config).unwrap();
    assert_eq!(preview.total, 2);
    assert_eq!(preview.affected, 1);
    import_config.preview_digest = Some(preview.digest.clone());
    assert!(store.commit_anki(import_config.clone()).is_err());
    import_config.skip_affected = true;
    let preview = store.preview_anki(&import_config).unwrap();
    import_config.preview_digest = Some(preview.digest);
    let result = store.commit_anki(import_config).unwrap();
    assert_eq!(result.skipped, 1);
    assert_eq!(result.imported, 1);

    let corrupt = dir.path().join("broken.apkg");
    fs::write(&corrupt, b"not a zip archive").unwrap();
    let config = config(&mut store, &corrupt, "skip");
    assert!(store.preview_anki(&config).is_err());
}

#[test]
fn missing_source_deck_and_template_are_fatal_package_errors() {
    let (dir, mut store) = setup();
    let missing_deck = dir.path().join("missing-deck.apkg");
    create_legacy(&missing_deck, &[(0, "No source deck", 999)], false);
    let missing_deck_config = config(&mut store, &missing_deck, "skip");
    assert!(store.preview_anki(&missing_deck_config).is_err());

    let missing_template = dir.path().join("missing-template.apkg");
    create_legacy(&missing_template, &[(99, "No source template", 1)], false);
    let missing_template_config = config(&mut store, &missing_template, "skip");
    assert!(store.preview_anki(&missing_template_config).is_err());
}

#[test]
fn modern_zstd_protobuf_package_imports_audio() {
    let (dir, mut store) = setup();
    let package = dir.path().join("modern.apkg");
    modern(&package);
    let mut config = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&config).unwrap();
    assert_eq!(preview.total, 1);
    assert_eq!(preview.audio_files, 1);
    config.preview_digest = Some(preview.digest);
    assert_eq!(store.commit_anki(config).unwrap().imported, 1);
    assert!(store.integrity().unwrap().healthy);
}

#[test]
fn failed_import_rolls_back_decks_cards_media_and_source_identity() {
    let (dir, mut store) = setup();
    let package = dir.path().join("rollback.apkg");
    create_legacy(&package, &[(0, "First", 1), (2, "Second", 1)], true);
    let mut config = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&config).unwrap();
    assert_eq!(preview.affected, 0);
    config.preview_digest = Some(preview.digest);
    store.conn.execute_batch("CREATE TRIGGER fail_second_anki_source BEFORE INSERT ON anki_sources WHEN NEW.ordinal=2 BEGIN SELECT RAISE(ABORT,'forced rollback'); END;").unwrap();
    assert!(store.commit_anki(config).is_err());
    assert!(store.decks().unwrap().is_empty());
    assert_eq!(store.browse(BrowseQuery::default()).unwrap().total, 0);
    assert_eq!(
        store
            .conn
            .query_row("SELECT count(*) FROM anki_sources", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .conn
            .query_row("SELECT count(*) FROM media", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(fs::read_dir(store.media_dir()).unwrap().count(), 0);
}

#[test]
fn updating_a_deleted_imported_note_is_reported_as_affected() {
    let (dir, mut store) = setup();
    let package = dir.path().join("deleted.apkg");
    create_legacy(&package, &[(0, "Imported", 1)], true);
    let mut initial = config(&mut store, &package, "skip");
    let preview = store.preview_anki(&initial).unwrap();
    initial.preview_digest = Some(preview.digest);
    store.commit_anki(initial).unwrap();
    let card = store
        .browse(BrowseQuery::default())
        .unwrap()
        .cards
        .remove(0);
    store
        .bulk(BulkInput {
            ids: vec![card.id],
            action: "delete".into(),
            value: None,
        })
        .unwrap();

    let update = config(&mut store, &package, "update");
    let preview = store.preview_anki(&update).unwrap();
    assert_eq!(preview.affected, 1);
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.contains("Restore the previously imported card"))
    );
    assert_eq!(
        store
            .conn
            .query_row("SELECT count(*) FROM anki_sources", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
}

#[test]
#[ignore = "large reference-deck acceptance; run with TALA_REFERENCE_APKG"]
fn reference_deck_import_backup_and_reimport_acceptance() {
    let path = std::env::var_os("TALA_REFERENCE_APKG")
        .expect("set TALA_REFERENCE_APKG to a disposable reference package");
    let path = Path::new(&path);
    let (dir, mut store) = setup();
    let mut config = config(&mut store, path, "skip");
    let preview = store.preview_anki(&config).unwrap();
    eprintln!(
        "reference preview: audio_files={}, audio_assets={}, affected={}, warnings={:?}, issues={:?}",
        preview.audio_files,
        preview.audio_assets,
        preview.affected,
        preview.warnings,
        preview.issues
    );
    assert_eq!(preview.total, 1432);
    assert_eq!(preview.decks.len(), 69);
    assert_eq!(
        preview.decks.iter().filter(|deck| deck.cards > 0).count(),
        68
    );
    assert_eq!(
        preview.decks.iter().filter(|deck| deck.cards == 0).count(),
        1
    );
    assert_eq!(preview.audio_files, 1409);
    assert_eq!(preview.audio_assets, 1406);
    assert_eq!(preview.affected, 0);
    assert!(!preview.warnings.is_empty());
    assert_eq!(
        tala_lib::content::plain_text(&preview.cards[0].front),
        "Think of possible Tagalog translations of:\n\nGood morning. (formal)"
    );
    let answer = tala_lib::content::plain_text(&preview.cards[0].back);
    assert!(answer.starts_with("Magandang umaga po."), "{answer}");
    assert!(answer.contains("Tagalog.com:"), "{answer}");
    config.preview_digest = Some(preview.digest);
    let imported = store.commit_anki(config.clone()).unwrap();
    assert_eq!(imported.imported, 1432);
    let media_refs: u32 = store
        .conn
        .query_row("SELECT count(*) FROM note_media", [], |r| r.get(0))
        .unwrap();
    assert_eq!(media_refs, 1430);
    let without_media: u32 = store
        .conn
        .query_row(
            "SELECT count(*) FROM notes n WHERE NOT EXISTS(SELECT 1 FROM note_media nm WHERE nm.note_id=n.id)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(without_media, 2);
    let audio_assets: u32 = store
        .conn
        .query_row(
            "SELECT count(*) FROM media WHERE mime LIKE 'audio/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audio_assets, 1406);
    let backup = store.create_backup(false).unwrap();
    let reread = store.preview_anki(&config).unwrap();
    assert_eq!(reread.duplicates, 1432);
    config.preview_digest = Some(reread.digest);
    assert_eq!(store.commit_anki(config).unwrap().skipped, 1432);
    store.restore_named(&backup.name).unwrap();
    assert_eq!(store.browse(BrowseQuery::default()).unwrap().total, 1432);
    assert!(store.integrity().unwrap().healthy);
    assert!(dir.path().join("collection").exists());
}
