//! Bounded Anki package reader. Source HTML is converted to Tala's canonical
//! document tree; source scripts and styles are never evaluated.
use crate::{
    content::{plain_text, validate_document},
    error::{AppError, Result},
    files::atomic_write,
    models::*,
    store::Store,
};
use prost::Message;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use scraper::{ElementRef, Html};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use zip::ZipArchive;

const MAX_PACKAGE: u64 = 1024 * 1024 * 1024;
const MAX_DATABASE: u64 = 256 * 1024 * 1024;
const MAX_MEDIA: u64 = 20 * 1024 * 1024;
const MAX_EXTRACTED: u64 = 4 * 1024 * 1024 * 1024;

// Minimal wire readers for the documented package versions. Unknown protobuf
// fields are skipped by prost; these structs are not Anki runtime interfaces.
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

#[derive(Clone)]
struct Template {
    question: String,
    answer: String,
}
struct Model {
    kind: u32,
    custom_style: bool,
    fields: Vec<String>,
    templates: HashMap<u32, Template>,
}
#[derive(Clone)]
struct PreparedMedia {
    id: String,
    path: PathBuf,
    extension: String,
    mime: String,
}
struct Mapping {
    entry: String,
    size: Option<u32>,
    sha1: Option<Vec<u8>>,
}
struct Package {
    archive: ZipArchive<File>,
    modern: bool,
    directory: TempDir,
    mapping: HashMap<String, Mapping>,
    cache: HashMap<String, Result<PreparedMedia>>,
    warnings: BTreeSet<String>,
    extracted: u64,
}
struct Converted {
    guid: String,
    ordinal: u32,
    existing: Option<String>,
    card: AnkiPreviewCard,
}
struct Parsed {
    package: Package,
    cards: Vec<Converted>,
    preview: AnkiImportPreview,
}

fn corrupt(message: impl Into<String>) -> AppError {
    AppError::invalid(format!("The Anki package is invalid: {}", message.into()))
}
fn decode<M: Message + Default>(bytes: &[u8]) -> Result<M> {
    M::decode(bytes).map_err(|_| corrupt("damaged package metadata."))
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 1024
        && !name.contains(['/', '\\', ':'])
        && name != "."
        && name != ".."
        && !name.chars().any(char::is_control)
}
fn read_entry(
    archive: &mut ZipArchive<File>,
    name: &str,
    limit: u64,
    compressed: bool,
    extracted: Option<&mut u64>,
) -> Result<Vec<u8>> {
    let entry = archive
        .by_name(name)
        .map_err(|_| corrupt(format!("missing {name}.")))?;
    if entry.size() > limit {
        return Err(corrupt(format!("{name} exceeds its size limit.")));
    }
    let mut reader: Box<dyn Read + '_> = if compressed {
        let mut decoder = zstd::stream::read::Decoder::new(entry)
            .map_err(|_| corrupt("damaged compressed entry."))?;
        decoder
            .window_log_max(27)
            .map_err(|_| corrupt("unsupported compression window."))?;
        Box::new(decoder)
    } else {
        Box::new(entry)
    };
    let mut bytes = Vec::new();
    let result = reader.by_ref().take(limit + 1).read_to_end(&mut bytes);
    if let Some(extracted) = extracted {
        *extracted = extracted.saturating_add(bytes.len() as u64);
    }
    result.map_err(|_| corrupt("an entry could not be decoded."))?;
    if bytes.len() as u64 > limit {
        return Err(corrupt(format!("{name} expands beyond its size limit.")));
    }
    Ok(bytes)
}
fn legacy_media_map(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    struct UniqueMap;
    impl<'de> serde::de::Visitor<'de> for UniqueMap {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a unique media map")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                if entries.len() >= 100_000 || entries.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate or oversized media map"));
                }
            }
            Ok(entries)
        }
    }
    use serde::Deserializer;
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let entries = parser
        .deserialize_map(UniqueMap)
        .map_err(|_| corrupt("invalid or duplicate media mapping."))?;
    parser
        .end()
        .map_err(|_| corrupt("invalid media mapping."))?;
    Ok(entries)
}
fn digest(path: &Path, config: &AnkiImportConfig) -> Result<String> {
    let mut file = File::open(path)?;
    if file.metadata()?.len() > MAX_PACKAGE {
        return Err(corrupt("packages must be no larger than 1 GB."));
    }
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > MAX_PACKAGE {
            return Err(corrupt("packages must be no larger than 1 GB."));
        }
        hash.update(&buf[..n]);
    }
    let mut config = config.clone();
    config.preview_digest = None;
    hash.update(serde_json::to_vec(&config)?);
    Ok(hex::encode(hash.finalize()))
}

impl Package {
    fn open(path: &Path) -> Result<(Self, Connection)> {
        if fs::metadata(path)?.len() > MAX_PACKAGE {
            return Err(corrupt("packages must be no larger than 1 GB."));
        }
        let mut archive = ZipArchive::new(File::open(path)?)
            .map_err(|_| corrupt("not a readable ZIP package."))?;
        if archive.len() > 100_000 {
            return Err(corrupt("too many archive entries."));
        }
        let mut seen = HashSet::new();
        let mut total = 0u64;
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            if !safe_name(entry.name())
                || entry.is_dir()
                || entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
                || !seen.insert(entry.name().to_owned())
            {
                return Err(corrupt("unsafe or duplicate archive paths."));
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| corrupt("invalid extracted size."))?;
            if total > MAX_EXTRACTED {
                return Err(corrupt("the package expands beyond 4 GB."));
            }
        }
        let version = if seen.contains("meta") {
            decode::<Meta>(&read_entry(&mut archive, "meta", 1024, false, None)?)?.version
        } else if seen.contains("collection.anki21") {
            2
        } else {
            1
        };
        let database = match version {
            1 => "collection.anki2",
            2 => "collection.anki21",
            3 => "collection.anki21b",
            _ => return Err(corrupt("this package version is not supported.")),
        };
        let bytes = read_entry(&mut archive, database, MAX_DATABASE, version == 3, None)?;
        if !bytes.starts_with(b"SQLite format 3\0") {
            return Err(corrupt("the collection is not a SQLite database."));
        }
        let directory = tempfile::tempdir()?;
        let dbpath = directory.path().join("source.sqlite3");
        let mut file = File::create(&dbpath)?;
        file.write_all(&bytes)?;
        drop(file);
        let conn = Connection::open_with_flags(dbpath, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.execute_batch("PRAGMA trusted_schema=OFF;")?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(corrupt("the source database is damaged."));
        }
        let modern_tables: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='notetypes' AND type='table')",
            [],
            |r| r.get(0),
        )?;
        let required = if modern_tables {
            vec![
                "notes",
                "cards",
                "notetypes",
                "fields",
                "templates",
                "decks",
            ]
        } else {
            vec!["notes", "cards", "col"]
        };
        for table in required {
            let sql: Option<String> = conn
                .query_row(
                    "SELECT sql FROM sqlite_schema WHERE name=?1 AND type='table'",
                    [table],
                    |r| r.get(0),
                )
                .optional()?;
            if sql.is_none_or(|sql| sql.to_ascii_lowercase().contains("virtual")) {
                return Err(corrupt("required source tables are missing."));
            }
        }
        let data = if seen.contains("media") {
            read_entry(&mut archive, "media", 16 * 1024 * 1024, version == 3, None)?
        } else if version == 3 {
            MediaEntries { entries: vec![] }.encode_to_vec()
        } else {
            b"{}".to_vec()
        };
        let mut mapping = HashMap::new();
        if version == 3 {
            for (index, entry) in decode::<MediaEntries>(&data)?
                .entries
                .into_iter()
                .enumerate()
            {
                if !safe_name(&entry.name)
                    || entry.sha1.len() != 20
                    || mapping.contains_key(&entry.name)
                {
                    return Err(corrupt("invalid media mapping."));
                }
                mapping.insert(
                    entry.name,
                    Mapping {
                        entry: index.to_string(),
                        size: Some(entry.size),
                        sha1: Some(entry.sha1),
                    },
                );
            }
        } else {
            let entries = legacy_media_map(&data)?;
            for (index, name) in entries {
                if !safe_name(&name) || index.parse::<u32>().is_err() || mapping.contains_key(&name)
                {
                    return Err(corrupt("invalid media mapping."));
                }
                mapping.insert(
                    name,
                    Mapping {
                        entry: index,
                        size: None,
                        sha1: None,
                    },
                );
            }
        }
        let extracted = bytes.len() as u64 + data.len() as u64;
        Ok((
            Self {
                archive,
                modern: version == 3,
                directory,
                mapping,
                cache: HashMap::new(),
                warnings: BTreeSet::new(),
                extracted,
            },
            conn,
        ))
    }
    fn media(&mut self, name: &str, audio: bool) -> Result<PreparedMedia> {
        if !safe_name(name) {
            return Err(AppError::invalid(
                "A media reference contains an unsafe filename.",
            ));
        }
        if let Some(cached) = self.cache.get(name) {
            let cached = cached.clone()?;
            if cached.mime.starts_with("audio/") != audio {
                return Err(AppError::invalid("A media reference has the wrong type."));
            }
            return Ok(cached);
        }
        let result = (|| {
            let mapping = self
                .mapping
                .get(name)
                .ok_or_else(|| AppError::invalid(format!("Missing media: {name}")))?;
            let remaining = MAX_EXTRACTED.saturating_sub(self.extracted);
            let bytes = read_entry(
                &mut self.archive,
                &mapping.entry,
                MAX_MEDIA.min(remaining),
                self.modern,
                Some(&mut self.extracted),
            )?;
            if self.extracted > MAX_EXTRACTED {
                return Err(corrupt("the package expands beyond 4 GB."));
            }
            if mapping
                .size
                .is_some_and(|size| size as usize != bytes.len())
            {
                return Err(AppError::invalid(format!("Media size mismatch: {name}")));
            }
            if let Some(expected) = &mapping.sha1 {
                use sha1::Digest;
                if sha1::Sha1::digest(&bytes).as_slice() != expected {
                    return Err(AppError::invalid(format!(
                        "Media checksum mismatch: {name}"
                    )));
                }
            }
            let info = crate::media::validate(&bytes, audio)
                .map_err(|e| AppError::invalid(format!("{name}: {}", e.message)))?;
            let id = format!("{}.{}", hex::encode(Sha256::digest(&bytes)), info.extension);
            let path = self.directory.path().join(&id);
            if !path.exists() {
                atomic_write(&path, &bytes)?;
            }
            Ok(PreparedMedia {
                id,
                path,
                extension: info.extension.into(),
                mime: info.mime.into(),
            })
        })();
        self.cache.insert(name.into(), result.clone());
        result
    }
}

fn models(conn: &Connection) -> Result<HashMap<i64, Model>> {
    let modern: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='notetypes' AND type='table')",
        [],
        |r| r.get(0),
    )?;
    let mut result = HashMap::new();
    if modern {
        for row in conn
            .prepare("SELECT id,config FROM notetypes")?
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
        {
            let (id, config) = row?;
            let config = decode::<NotetypeConfig>(&config)?;
            let fields = conn
                .prepare("SELECT name FROM fields WHERE ntid=?1 ORDER BY ord")?
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut templates = HashMap::new();
            for row in conn
                .prepare("SELECT ord,config FROM templates WHERE ntid=?1 ORDER BY ord")?
                .query_map([id], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?)))?
            {
                let (ord, bytes) = row?;
                let template = decode::<TemplateConfig>(&bytes)?;
                templates.insert(
                    ord,
                    Template {
                        question: template.question,
                        answer: template.answer,
                    },
                );
            }
            result.insert(
                id,
                Model {
                    kind: config.kind,
                    custom_style: !config.css.trim().is_empty(),
                    fields,
                    templates,
                },
            );
        }
    } else {
        let raw: String = conn.query_row("SELECT models FROM col LIMIT 1", [], |r| r.get(0))?;
        let all: BTreeMap<String, Value> =
            serde_json::from_str(&raw).map_err(|_| corrupt("damaged note types."))?;
        for (id, value) in all {
            let id = id.parse().map_err(|_| corrupt("invalid note type ID."))?;
            let fields = value["flds"]
                .as_array()
                .ok_or_else(|| corrupt("invalid note fields."))?;
            let mut ordered = fields
                .iter()
                .map(|f| {
                    Ok((
                        f["ord"]
                            .as_u64()
                            .ok_or_else(|| corrupt("invalid field ordinal."))?,
                        f["name"]
                            .as_str()
                            .ok_or_else(|| corrupt("invalid field name."))?
                            .to_owned(),
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            ordered.sort_by_key(|f| f.0);
            let mut templates = HashMap::new();
            for t in value["tmpls"]
                .as_array()
                .ok_or_else(|| corrupt("invalid templates."))?
            {
                let ordinal = t["ord"]
                    .as_u64()
                    .and_then(|x| u32::try_from(x).ok())
                    .ok_or_else(|| corrupt("invalid template ordinal."))?;
                templates.insert(
                    ordinal,
                    Template {
                        question: t["qfmt"].as_str().unwrap_or("").into(),
                        answer: t["afmt"].as_str().unwrap_or("").into(),
                    },
                );
            }
            result.insert(
                id,
                Model {
                    kind: value["type"].as_u64().unwrap_or(1) as u32,
                    custom_style: value["css"]
                        .as_str()
                        .is_some_and(|css| !css.trim().is_empty()),
                    fields: ordered.into_iter().map(|(_, n)| n).collect(),
                    templates,
                },
            );
        }
    }
    Ok(result)
}
fn source_decks(conn: &Connection) -> Result<HashMap<i64, String>> {
    let modern: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='decks' AND type='table')",
        [],
        |r| r.get(0),
    )?;
    if modern {
        Ok(conn
            .prepare("SELECT id,name FROM decks")?
            .query_map([], |r| {
                Ok((r.get(0)?, r.get::<_, String>(1)?.replace('\u{1f}', "::")))
            })?
            .collect::<rusqlite::Result<_>>()?)
    } else {
        let raw: String = conn.query_row("SELECT decks FROM col LIMIT 1", [], |r| r.get(0))?;
        let all: BTreeMap<String, Value> = serde_json::from_str(&raw)?;
        all.into_iter()
            .map(|(id, d)| {
                Ok((
                    id.parse().map_err(|_| corrupt("invalid deck ID."))?,
                    d["name"]
                        .as_str()
                        .ok_or_else(|| corrupt("invalid deck name."))?
                        .into(),
                ))
            })
            .collect()
    }
}

fn expand(
    template: &str,
    fields: &HashMap<String, String>,
    question: bool,
) -> Result<(String, bool)> {
    if template.len() > 1_000_000 {
        return Err(AppError::invalid(
            "A template exceeds the content size limit.",
        ));
    }
    let mut result = String::new();
    let mut remaining = template;
    let mut active = vec![true];
    let mut sections = Vec::new();
    let mut typed = false;
    while let Some(start) = remaining.find("{{") {
        if *active.last().unwrap() {
            result.push_str(&remaining[..start]);
        }
        let tail = &remaining[start + 2..];
        let end = tail
            .find("}}")
            .ok_or_else(|| AppError::invalid("Unclosed template field."))?;
        let token = tail[..end].trim();
        remaining = &tail[end + 2..];
        if let Some(name) = token.strip_prefix('#').or_else(|| token.strip_prefix('^')) {
            let value = fields
                .get(name)
                .ok_or_else(|| AppError::invalid(format!("Unknown template field: {name}")))?;
            let enabled = if token.starts_with('^') {
                value.trim().is_empty()
            } else {
                !value.trim().is_empty()
            };
            if active.len() > 30 {
                return Err(AppError::invalid("Template nesting is too deep."));
            }
            active.push(*active.last().unwrap() && enabled);
            sections.push(name.to_string());
        } else if let Some(name) = token.strip_prefix('/') {
            if sections.pop().as_deref() != Some(name) {
                return Err(AppError::invalid("Mismatched template section."));
            }
            active.pop();
        } else if *active.last().unwrap() {
            if token == "FrontSide" {
                continue;
            }
            let (filter, name) = token.split_once(':').unwrap_or(("", token));
            if name == "FrontSide" && matches!(filter, "" | "text") {
                continue;
            }
            let field = fields.get(name).ok_or_else(|| {
                AppError::invalid(format!("Unknown template field or filter: {token}"))
            })?;
            match filter {
                "" => result.push_str(field),
                "text" => {
                    let html = Html::parse_fragment(field);
                    result.push_str(&escape_html(
                        &html.root_element().text().collect::<String>(),
                    ));
                }
                "type" => {
                    typed = true;
                    if !question {
                        result.push_str(field);
                    }
                }
                _ => {
                    return Err(AppError::invalid(format!(
                        "Unsupported template filter: {filter}"
                    )));
                }
            }
        }
        if result.len() > 1_000_000 {
            return Err(AppError::invalid("Expanded content exceeds 1 MB."));
        }
    }
    if active.len() != 1 {
        return Err(AppError::invalid("Unclosed template section."));
    }
    result.push_str(remaining);
    Ok((result, typed))
}
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn decode_filename(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(n) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
        {
            result.push(n);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).unwrap_or_else(|_| s.into())
}

#[derive(Default)]
struct Document {
    blocks: Vec<Value>,
    inline: Vec<Value>,
}
impl Document {
    fn flush(&mut self) {
        if !self.inline.is_empty() {
            self.blocks
                .push(json!({"type":"paragraph","content":std::mem::take(&mut self.inline)}));
        }
    }
    fn text(&mut self, text: &str, marks: &[Value], package: &mut Package) -> Result<()> {
        let mut remaining = text;
        while let Some(start) = remaining.find("[sound:") {
            self.literal(&remaining[..start], marks);
            let tail = &remaining[start + 7..];
            let end = tail
                .find(']')
                .ok_or_else(|| AppError::invalid("Unclosed audio reference."))?;
            self.media(tail[..end].trim(), true, package)?;
            remaining = &tail[end + 1..];
        }
        self.literal(remaining, marks);
        Ok(())
    }
    fn literal(&mut self, text: &str, marks: &[Value]) {
        if !text.is_empty() {
            let mut node = json!({"type":"text","text":text});
            if !marks.is_empty() {
                node["marks"] = json!(marks);
            }
            self.inline.push(node);
        }
    }
    fn media(&mut self, name: &str, audio: bool, package: &mut Package) -> Result<()> {
        let m = package.media(name, audio)?;
        self.flush();
        self.blocks.push(if audio {
            json!({"type":"audio","attrs":{"mediaId":m.id,"label":name}})
        } else {
            json!({"type":"image","attrs":{"mediaId":m.id,"alt":"","title":""}})
        });
        Ok(())
    }
    fn element(
        &mut self,
        el: ElementRef<'_>,
        marks: &[Value],
        package: &mut Package,
        depth: u32,
        count: &mut usize,
    ) -> Result<()> {
        *count += 1;
        if depth > 30 || *count > 10000 {
            return Err(AppError::invalid("HTML content is too complex."));
        }
        let tag = el.value().name();
        if ["script", "style", "iframe", "object", "embed"].contains(&tag) {
            package.warnings.insert("Scripts, embedded content, and custom styles were omitted; cards use Tala's presentation.".into());
            return Ok(());
        }
        if el
            .value()
            .attrs()
            .any(|(key, _)| key == "style" || key.starts_with("on"))
        {
            package
                .warnings
                .insert("Custom styling and event handlers were omitted.".into());
        }
        if ["video", "canvas"].contains(&tag) {
            return Err(AppError::invalid(
                "Video and image occlusion are not supported.",
            ));
        }
        if tag == "img" {
            let src = el.value().attr("src").unwrap_or("");
            return self.media(&decode_filename(src), false, package);
        }
        if tag == "audio" {
            let src = el
                .value()
                .attr("src")
                .or_else(|| {
                    el.children()
                        .filter_map(ElementRef::wrap)
                        .find_map(|source| source.value().attr("src"))
                })
                .unwrap_or("");
            return self.media(&decode_filename(src), true, package);
        }
        if tag == "br" {
            self.inline.push(json!({"type":"hardBreak"}));
            return Ok(());
        }
        if tag == "hr" {
            self.flush();
            self.blocks.push(json!({"type":"horizontalRule"}));
            return Ok(());
        }
        if ["ul", "ol"].contains(&tag) {
            self.flush();
            let mut items = Vec::new();
            for child in el
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|el| el.value().name() == "li")
            {
                let mut doc = Document::default();
                doc.element(child, marks, package, depth + 1, count)?;
                doc.flush();
                if doc.blocks.first().is_none_or(|n| n["type"] != "paragraph") {
                    doc.blocks.insert(0, json!({"type":"paragraph"}));
                }
                items.push(json!({"type":"listItem","content":doc.blocks}));
            }
            if !items.is_empty() {
                let mut node =
                    json!({"type":if tag=="ul"{"bulletList"}else{"orderedList"},"content":items});
                if tag == "ol" {
                    node["attrs"] = json!({"start":el.value().attr("start").and_then(|s|s.parse::<u32>().ok()).unwrap_or(1)});
                }
                self.blocks.push(node);
            }
            return Ok(());
        }
        if tag == "blockquote" {
            self.flush();
            let mut doc = Document::default();
            for child in el.children() {
                match child.value() {
                    scraper::Node::Text(text) => doc.text(text, marks, package)?,
                    scraper::Node::Element(_) => doc.element(
                        ElementRef::wrap(child).unwrap(),
                        marks,
                        package,
                        depth + 1,
                        count,
                    )?,
                    _ => {}
                }
            }
            doc.flush();
            if !doc.blocks.is_empty() {
                self.blocks
                    .push(json!({"type":"blockquote","content":doc.blocks}));
            }
            return Ok(());
        }
        if tag == "pre" {
            self.flush();
            self.blocks.push(json!({"type":"codeBlock","content":[{"type":"text","text":el.text().collect::<String>()}]}));
            return Ok(());
        }
        let block = [
            "p",
            "div",
            "section",
            "table",
            "tr",
            "li",
            "blockquote",
            "pre",
            "h1",
            "h2",
            "h3",
            "h4",
            "h5",
            "h6",
        ]
        .contains(&tag);
        if block {
            self.flush();
        }
        let first_block = self.blocks.len();
        let mut next = marks.to_vec();
        let mark = match tag {
            "b" | "strong" => Some("bold"),
            "i" | "em" => Some("italic"),
            "u" => Some("underline"),
            "s" | "strike" | "del" => Some("strike"),
            "sub" => Some("subscript"),
            "sup" => Some("superscript"),
            "code" => Some("code"),
            _ => None,
        };
        if let Some(mark) = mark {
            let mark = json!({"type":mark});
            if !next.contains(&mark) {
                next.push(mark);
            }
        }
        if tag == "a"
            && let Some(href) = el.value().attr("href")
        {
            if href.starts_with("https://")
                || href.starts_with("http://")
                || href.starts_with("mailto:")
            {
                next.push(json!({"type":"link","attrs":{"href":href}}));
            } else {
                package
                    .warnings
                    .insert("Unsupported link destinations were omitted.".into());
            }
        }
        for child in el.children() {
            match child.value() {
                scraper::Node::Text(text) => self.text(text, &next, package)?,
                scraper::Node::Element(_) => self.element(
                    ElementRef::wrap(child).unwrap(),
                    &next,
                    package,
                    depth + 1,
                    count,
                )?,
                _ => {}
            }
        }
        if block {
            self.flush();
        }
        if let Some(level) = tag
            .strip_prefix('h')
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|level| (1..=6).contains(level))
        {
            for node in &mut self.blocks[first_block..] {
                if node["type"] == "paragraph" {
                    node["type"] = json!("heading");
                    node["attrs"] = json!({"level":level});
                }
            }
        }
        Ok(())
    }
}
fn document(html: &str, package: &mut Package) -> Result<Value> {
    if html.len() > 1_000_000 {
        return Err(AppError::invalid("Card content exceeds 1 MB."));
    }
    let fragment = Html::parse_fragment(html);
    let mut doc = Document::default();
    doc.element(fragment.root_element(), &[], package, 0, &mut 0)?;
    doc.flush();
    // Whitespace between block elements should not become empty-looking cards.
    doc.blocks.retain(|node| {
        node["type"] != "paragraph"
            || !plain_text(node).trim().is_empty()
            || node["content"].as_array().is_some_and(|children| {
                children
                    .iter()
                    .any(|n| n["type"] != "text" && n["type"] != "hardBreak")
            })
    });
    let (canonical, _) = validate_document(&json!({"type":"doc","content":doc.blocks}))?;
    Ok(canonical)
}

impl Store {
    fn parse_anki(&self, config: &AnkiImportConfig) -> Result<Parsed> {
        if !["skip", "update", "separate"].contains(&config.duplicates.as_str()) {
            return Err(AppError::invalid(
                "Choose how duplicates should be handled.",
            ));
        }
        if let Some(parent) = &config.parent_id {
            self.assert_deck(parent)?;
        }
        let path = self.granted_path(&config.path_token, "import")?;
        let initial = digest(&path, config)?;
        let (mut package, conn) = Package::open(&path)?;
        let models = models(&conn)?;
        let decks = source_decks(&conn)?;
        let total: u32 = conn.query_row("SELECT count(*) FROM cards", [], |r| r.get(0))?;
        if total == 0 || total > 500_000 {
            return Err(corrupt("packages must contain 1–500,000 cards."));
        }
        let own = self.deck_records()?;
        let prefix = config
            .parent_id
            .as_ref()
            .and_then(|id| own.iter().find(|d| &d.id == id))
            .map(|d| format!("{}::", d.path))
            .unwrap_or_default();
        let mut cards = Vec::new();
        let mut issues = Vec::new();
        let mut paths = BTreeMap::<String, u32>::new();
        let mut identities = HashSet::new();
        let mut affected = 0;
        let mut duplicates = 0;
        let rows=conn.prepare("SELECT c.id,c.ord,CASE WHEN c.odid<>0 THEN c.odid ELSE c.did END,n.guid,n.mid,n.flds,n.tags FROM cards c JOIN notes n ON n.id=c.nid ORDER BY n.id,c.ord")?
            .query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,u32>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() != total as usize {
            return Err(corrupt("some cards have missing notes."));
        }
        for (index, (_id, ordinal, deck_id, guid, model_id, fields, tags)) in
            rows.into_iter().enumerate()
        {
            if !decks.contains_key(&deck_id) {
                return Err(corrupt("a card refers to a missing deck."));
            }
            let source_model = models
                .get(&model_id)
                .ok_or_else(|| corrupt("a note refers to a missing note type."))?;
            if source_model.kind == 0 && !source_model.templates.contains_key(&ordinal) {
                return Err(corrupt("a card refers to a missing template."));
            }
            if fields.split('').count() != source_model.fields.len() {
                return Err(corrupt("a note has an inconsistent field count."));
            }
            if guid.is_empty() || guid.len() > 200 {
                return Err(corrupt("invalid source note GUID."));
            }
            if !identities.insert((guid.clone(), ordinal)) {
                return Err(corrupt("duplicate source card identities."));
            }
            let mut card = AnkiPreviewCard {
                number: index as u32 + 1,
                deck_path: String::new(),
                front: json!({"type":"doc","content":[]}),
                back: json!({"type":"doc","content":[]}),
                behavior: Behavior::Normal,
                tags: vec![],
                duplicate: false,
                error: None,
            };
            let existing: Option<String> = self
                .conn
                .query_row(
                    "SELECT note_id FROM anki_sources WHERE guid=?1 AND ordinal=?2",
                    params![guid, ordinal],
                    |r| r.get(0),
                )
                .optional()?;
            card.duplicate = existing.is_some();
            if card.duplicate {
                duplicates += 1;
            }
            let converted = (|| -> Result<()> {
                let name = decks
                    .get(&deck_id)
                    .ok_or_else(|| AppError::invalid("The card's source deck is missing."))?;
                let segments: Vec<_> = name.split("::").collect();
                if segments.len() > 30
                    || segments
                        .iter()
                        .any(|s| s.trim().is_empty() || s.len() > 200)
                {
                    return Err(AppError::invalid(
                        "A deck path is empty, too long, or nested more than 30 levels.",
                    ));
                }
                card.deck_path = format!("{prefix}{name}");
                let model = models
                    .get(&model_id)
                    .ok_or_else(|| AppError::invalid("The card's note type is missing."))?;
                if model.custom_style {
                    package.warnings.insert(
                        "Anki note-type styling was omitted; cards use Tala's presentation.".into(),
                    );
                }
                if model.kind != 0 {
                    return Err(AppError::invalid(
                        "Cloze and image occlusion cards are not supported.",
                    ));
                }
                let template = model
                    .templates
                    .get(&ordinal)
                    .ok_or_else(|| AppError::invalid("The card's template is missing."))?;
                let values: Vec<_> = fields.split('\u{1f}').collect();
                if values.len() != model.fields.len() {
                    return Err(AppError::invalid("The note's field count is inconsistent."));
                }
                let mut values: HashMap<_, _> = model
                    .fields
                    .iter()
                    .cloned()
                    .zip(values.into_iter().map(str::to_owned))
                    .collect();
                values.insert("Deck".into(), name.clone());
                values.insert("Subdeck".into(), segments.last().unwrap().to_string());
                values.insert("Tags".into(), tags.clone());
                let (front, typed) = expand(&template.question, &values, true)?;
                let (back, _) = expand(&template.answer, &values, false)?;
                card.front = document(&front, &mut package)?;
                card.back = document(&back, &mut package)?;
                card.behavior = if typed {
                    Behavior::Typed
                } else {
                    Behavior::Normal
                };
                card.tags = tags
                    .split_whitespace()
                    .map(crate::content::tag_name)
                    .collect::<Result<Vec<_>>>()?;
                if card.tags.len() > 100 {
                    return Err(AppError::invalid("A note contains more than 100 tags."));
                }
                if config.duplicates == "update"
                    && let Some(id) = &existing
                    && self.note_card(id)?.deleted_at.is_some()
                {
                    return Err(AppError::invalid(
                        "Restore the previously imported card from Recently Deleted before updating it.",
                    ));
                }
                Ok(())
            })();
            if package.extracted > MAX_EXTRACTED {
                return Err(corrupt("the package expands beyond 4 GB."));
            }
            if let Err(error) = converted {
                affected += 1;
                issues.push(format!("Card {}: {}", card.number, error.message));
                card.error = Some(error.message);
            } else {
                *paths.entry(card.deck_path.clone()).or_default() += 1;
                let components: Vec<_> = card.deck_path.split("::").collect();
                let base = if prefix.is_empty() {
                    0
                } else {
                    prefix.split("::").count() - 1
                };
                for i in base + 1..components.len() {
                    paths.entry(components[..i].join("::")).or_default();
                }
            }
            cards.push(Converted {
                guid,
                ordinal,
                existing,
                card,
            });
        }
        let mut blocking_errors = Vec::new();
        let mut destinations = Vec::new();
        let mut selected_parents = HashMap::<String, Option<String>>::new();
        if let Some(parent) = &config.parent_id {
            selected_parents.insert(prefix.trim_end_matches("::").into(), Some(parent.clone()));
        }
        for (path, count) in paths {
            let parent_path = path
                .rsplit_once("::")
                .map(|(parent, _)| parent)
                .unwrap_or("");
            let matches: Vec<_> = own
                .iter()
                .filter(|d| {
                    d.path == path
                        && if parent_path.is_empty() {
                            d.parent_id.is_none()
                        } else {
                            selected_parents.get(parent_path).is_none_or(|parent| {
                                parent
                                    .as_ref()
                                    .is_some_and(|id| d.parent_id.as_ref() == Some(id))
                            })
                        }
                })
                .map(|d| d.id.clone())
                .collect();
            if let Some(id) = config.destinations.get(&path) {
                self.assert_deck(id)?;
                if !matches.contains(id) {
                    return Err(AppError::invalid(
                        "Choose an existing deck with the matching destination path.",
                    ));
                }
            } else if matches.len() > 1 {
                blocking_errors.push(format!("Choose which existing deck to use for {path}."));
            }
            if let Some(id) = config
                .destinations
                .get(&path)
                .cloned()
                .or_else(|| (matches.len() == 1).then(|| matches[0].clone()))
            {
                selected_parents.insert(path.clone(), Some(id));
            } else if matches.is_empty() {
                selected_parents.insert(path.clone(), None);
            }
            destinations.push(AnkiDestination {
                path,
                cards: count,
                matches,
            });
        }
        if digest(&path, config)? != initial {
            return Err(AppError::conflict(
                "The package changed during preview. Select it again.",
            ));
        }
        let successful: HashMap<_, _> = package
            .cache
            .values()
            .filter_map(|r| r.as_ref().ok())
            .map(|m| (m.id.as_str(), m.mime.as_str()))
            .collect();
        let preview = AnkiImportPreview {
            digest: initial,
            total,
            duplicates,
            affected,
            audio_files: package
                .cache
                .values()
                .filter(|r| r.as_ref().is_ok_and(|m| m.mime.starts_with("audio/")))
                .count() as u32,
            audio_assets: successful
                .values()
                .filter(|mime| mime.starts_with("audio/"))
                .count() as u32,
            image_files: package
                .cache
                .values()
                .filter(|r| r.as_ref().is_ok_and(|m| m.mime.starts_with("image/")))
                .count() as u32,
            decks: destinations,
            cards: cards.iter().take(50).map(|c| c.card.clone()).collect(),
            warnings: package.warnings.iter().cloned().collect(),
            issues,
            blocking_errors,
        };
        Ok(Parsed {
            package,
            cards,
            preview,
        })
    }
    pub fn preview_anki(&self, config: &AnkiImportConfig) -> Result<AnkiImportPreview> {
        Ok(self.parse_anki(config)?.preview)
    }
    pub fn commit_anki(&mut self, config: AnkiImportConfig) -> Result<ImportResult> {
        let parsed = self.parse_anki(&config)?;
        if config.preview_digest.as_deref() != Some(&parsed.preview.digest) {
            return Err(AppError::conflict(
                "Preview the current package and options before importing.",
            ));
        }
        if !parsed.preview.blocking_errors.is_empty()
            || parsed.preview.affected > 0 && !config.skip_affected
        {
            return Err(AppError::invalid(
                "Resolve destination choices or explicitly skip affected cards before importing.",
            ));
        }
        if parsed.preview.affected == parsed.preview.total {
            return Err(AppError::invalid(
                "The package contains no supported cards.",
            ));
        }
        let media_by_id: HashMap<_, _> = parsed
            .package
            .cache
            .values()
            .filter_map(|r| r.as_ref().ok())
            .map(|m| (m.id.clone(), m.clone()))
            .collect();
        let mut installed = Vec::new();
        let result=self.transaction(|store|{
            let mut destinations=HashMap::new();
            let mut ordered=parsed.preview.decks.clone();ordered.sort_by_key(|d|(d.path.split("::").count(),d.path.clone()));
            let own=store.deck_records()?;
            if let Some(parent)=&config.parent_id{let path=&own.iter().find(|d|&d.id==parent).ok_or_else(||AppError::invalid("The import parent was deleted."))?.path;destinations.insert(path.clone(),parent.clone());}
            let defaults=store.preferences()?.defaults;
            for destination in ordered {
                let existing=config.destinations.get(&destination.path).cloned().or_else(||destination.matches.first().cloned());
                let (parent_path,name)=destination.path.rsplit_once("::").unwrap_or(("",&destination.path));
                let parent_id=if parent_path.is_empty(){None}else{Some(destinations.get(parent_path).cloned().ok_or_else(||AppError::invalid("An import parent is missing."))?)};
                let id=if let Some(id)=existing{
                    let deck = own.iter().find(|d| d.id == id).ok_or_else(|| AppError::invalid("The destination deck was deleted. Preview again."))?;
                    if deck.parent_id != parent_id { return Err(AppError::invalid("Destination decks must belong to the selected parent branch. Preview again and choose consistent destinations.")); }
                    id
                }else{store.save_deck(DeckInput{id:None,name:name.into(),parent_id,cover:None,color:"violet".into(),settings:defaults.clone()})?};
                destinations.insert(destination.path,id);
            }
            let mut result=ImportResult{imported:0,updated:0,skipped:0};
            for converted in parsed.cards {
                if converted.card.error.is_some()||converted.existing.is_some()&&config.duplicates=="skip"{result.skipped+=1;continue;}
                let (_,mut refs)=validate_document(&converted.card.front)?;let (_,back)=validate_document(&converted.card.back)?;refs.extend(back);refs.sort();refs.dedup();
                for id in refs {
                    let media=media_by_id.get(&id).ok_or_else(||AppError::invalid("A staged media file is missing."))?;
                    let target=store.media_dir().join(&id);if !target.exists(){installed.push(target);}
                    store.install_media(&fs::read(&media.path)?,&media.extension,&media.mime)?;
                }
                let existing=if config.duplicates=="update"{converted.existing.clone()}else{None};
                let card=store.save_note(NoteInput{id:existing.clone(),deck_id:destinations[&converted.card.deck_path].clone(),front:converted.card.front,back:converted.card.back,behavior:converted.card.behavior,tags:converted.card.tags})?;
                if existing.is_some(){result.updated+=1;}else{result.imported+=1;}
                if converted.existing.is_none(){store.conn.execute("INSERT INTO anki_sources(guid,ordinal,note_id) VALUES (?1,?2,?3)",params![converted.guid,converted.ordinal,card.note_id])?;}
            }
            Ok(result)
        });
        if result.is_err() {
            for path in installed {
                let _ = fs::remove_file(path);
            }
        }
        result
    }
}
