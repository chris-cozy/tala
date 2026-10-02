import assert from "node:assert/strict";
import { writeFileSync, readFileSync, existsSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

async function command(action: string, payload?: unknown): Promise<any> {
  return browser.tauri.execute(
    ({ core }, request) => core.invoke("dispatch", { request }),
    { action, ...(payload === undefined ? {} : { payload }) },
  );
}

async function resize(width: number, height: number) {
  const ratio = await browser.execute(() => devicePixelRatio);
  await browser.setWindowSize(width * ratio, height * ratio);
  await browser.waitUntil(
    async () =>
      await browser.execute(
        (width, height) => innerWidth === width && innerHeight === height,
        width,
        height,
      ),
  );
}

async function selectOption(selector: string, value: string) {
  await $(selector).waitForDisplayed();
  // Embedded WKWebView's option click does not dispatch a select change event.
  await browser.execute(
    (selector, value) => {
      const element = document.querySelector(selector) as HTMLSelectElement;
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        "value",
      )!.set!.call(element, value);
      element.dispatchEvent(new Event("input", { bubbles: true }));
      element.dispatchEvent(new Event("change", { bubbles: true }));
    },
    selector,
    value,
  );
  assert.equal(await $(selector).getValue(), value);
}

async function caretAtEnd(selector: string) {
  await browser.execute((selector) => {
    const element = document.querySelector(selector) as HTMLElement;
    element.focus();
    const range = document.createRange();
    range.selectNodeContents(element);
    range.collapse(false);
    const selection = getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    document.dispatchEvent(new Event("selectionchange"));
  }, selector);
}

function makeWav(): Buffer {
  const sampleRate = 22050;
  const sampleCount = sampleRate * 5;
  const data = Buffer.alloc(sampleCount * 2);
  for (let i = 0; i < sampleCount; i++) {
    const envelope = Math.min(1, i / 400, (sampleCount - i) / 400);
    data.writeInt16LE(
      Math.round(
        Math.sin((2 * Math.PI * 440 * i) / sampleRate) * 5000 * envelope,
      ),
      i * 2,
    );
  }
  const wav = Buffer.alloc(44 + data.length);
  wav.write("RIFF", 0);
  wav.writeUInt32LE(wav.length - 8, 4);
  wav.write("WAVEfmt ", 8);
  wav.writeUInt32LE(16, 16);
  wav.writeUInt16LE(1, 20);
  wav.writeUInt16LE(1, 22);
  wav.writeUInt32LE(sampleRate, 24);
  wav.writeUInt32LE(sampleRate * 2, 28);
  wav.writeUInt16LE(2, 32);
  wav.writeUInt16LE(16, 34);
  wav.write("data", 36);
  wav.writeUInt32LE(data.length, 40);
  data.copy(wav, 44);
  return wav;
}

function createAnkiFixture(file: string, audio: Buffer) {
  writeFileSync(`${file}.wav`, audio);
  const python = String.raw`
import json, sqlite3, sys, zipfile
database, package, audio_path = sys.argv[1:]
db = sqlite3.connect(database)
db.executescript("""
CREATE TABLE col(id INTEGER, models TEXT, decks TEXT);
CREATE TABLE notes(id INTEGER PRIMARY KEY, guid TEXT, mid INTEGER, flds TEXT, tags TEXT);
CREATE TABLE cards(id INTEGER PRIMARY KEY, nid INTEGER, ord INTEGER, did INTEGER, odid INTEGER);
""")
models = {"1": {"type": 0, "flds": [{"name": "Front", "ord": 0}, {"name": "Back", "ord": 1}], "tmpls": [{"ord": 0, "qfmt": "{{Front}}", "afmt": "{{FrontSide}}<hr>{{Back}}"}]}}
decks = {"1": {"name": "UI package::Audio lesson"}}
db.execute("INSERT INTO col VALUES(1, ?, ?)", (json.dumps(models), json.dumps(decks)))
db.execute("INSERT INTO notes VALUES(10, 'ui-audio-guid', 1, ?, '')", ("Anki UI import prompt\x1f[sound:voice.wav]",))
db.execute("INSERT INTO cards VALUES(20, 10, 0, 1, 0)")
db.commit(); db.close()
with zipfile.ZipFile(package, "w", zipfile.ZIP_DEFLATED) as z:
    z.write(database, "collection.anki2")
    z.writestr("media", json.dumps({"0": "voice.wav"}))
    z.write(audio_path, "0")
`;
  const result = spawnSync(
    "python3",
    ["-c", python, `${file}.sqlite3`, file, `${file}.wav`],
    { encoding: "utf8" },
  );
  assert.equal(
    result.status,
    0,
    result.stderr || "Could not generate Anki fixture",
  );
}

function extractReferenceMp3(folder: string): string | null {
  const packagePath = process.env.TALA_REFERENCE_APKG;
  if (!packagePath || !existsSync(packagePath)) return null;
  const output = path.join(folder, "reference-audio.mp3");
  const python = String.raw`
import json, sys, zipfile
source, output = sys.argv[1:]
with zipfile.ZipFile(source) as package:
    mapping = json.loads(package.read("media"))
    clips = [(package.getinfo(name).file_size, name) for name, original in mapping.items() if original.lower().endswith(".mp3") and package.getinfo(name).file_size <= 2000000]
    if not clips: raise SystemExit("Reference APKG has no MP3 media")
    with open(output, "wb") as target: target.write(package.read(max(clips)[1]))
`;
  const result = spawnSync("python3", ["-c", python, packagePath, output], {
    encoding: "utf8",
  });
  assert.equal(
    result.status,
    0,
    result.stderr || "Could not extract reference MP3",
  );
  return output;
}

describe("Tala native collection", () => {
  before(async () => {
    await resize(1380, 900);
    await browser.execute(() => {
      (window as any).__talaErrors = [];
      window.addEventListener("error", (event) =>
        (window as any).__talaErrors.push(String(event.error ?? event.message)),
      );
      window.addEventListener("unhandledrejection", (event) =>
        (window as any).__talaErrors.push(String(event.reason)),
      );
      const original = console.error;
      console.error = (...args) => {
        (window as any).__talaErrors.push(
          args
            .map((value) =>
              value instanceof Error ? value.stack : String(value),
            )
            .join(" "),
        );
        original(...args);
      };
    });
  });
  it("starts empty and creates a deck through the interface", async () => {
    await $("h1").waitForDisplayed();
    const first = await command("bootstrap");
    assert.equal(first.decks.length, 0);
    assert.equal(first.today.studied, 0);
    await browser.saveScreenshot("artifacts/e2e/01-empty.png");
    await $("button*=Create").click();
    await $('[role="dialog"] input').setValue("Biology");
    await command("test_grant", {
      path: path.resolve("public/tala-mark.png"),
      purpose: "test-pick:image",
    });
    await $("button=Add cover").click();
    await $("button=Replace cover").waitForDisplayed();
    await $("button=Create deck").click();
    await browser.waitUntil(
      async () => (await command("bootstrap")).decks.length === 1,
    );
    assert.ok((await command("bootstrap")).decks[0].cover);
    await $("h1=Biology").waitForDisplayed();
    await browser.saveScreenshot("artifacts/e2e/02-deck.png");
  });

  it("authors and previews a real card, then persists it in SQLite", async () => {
    await $('nav button[title="Add card"]').click();
    await $('[aria-label="Front"][contenteditable="true"]').waitForDisplayed();
    await $('[aria-label="Front"][contenteditable="true"]').setValue(
      "What is the powerhouse of the cell?",
    );
    await $('[aria-label="Back"][contenteditable="true"]').setValue(
      "Mitochondria.",
    );
    await $('[aria-label="Add tags"]').setValue("biology");
    await browser.keys("Enter");
    await browser.saveScreenshot("artifacts/e2e/03-editor.png");
    await $("button=Preview").click();
    await $('[role="dialog"]').waitForDisplayed();
    assert.match(await $('[role="dialog"]').getText(), /powerhouse/);
    await $("button=Reveal answer").click();
    assert.match(await $('[role="dialog"]').getText(), /Mitochondria/);
    await $("button=Back to editor").click();
    await $("button*=Save card").click();
    await browser.waitUntil(
      async () =>
        (
          await command("browse", {
            search: "",
            deck: null,
            tag: null,
            state: null,
            leech: false,
            trash: false,
            sort: "created",
            descending: false,
            offset: 0,
            limit: 100,
          })
        ).total === 1,
    );
  });

  it("reveals, grades, and reverses a review against the native scheduler", async () => {
    await $('nav button[title="Today"]').click();
    await $("button*=Start").click();
    await $(".study-card").waitForDisplayed();
    assert.equal(await $$(".grade-button").length, 0);
    await browser.saveScreenshot("artifacts/e2e/04-study-front.png");
    await browser.keys("Space");
    await $(".grade-button").waitForDisplayed();
    assert.match(await $(".study-card").getText(), /Mitochondria/);
    await browser.saveScreenshot("artifacts/e2e/05-study-answer.png");
    const before = await command("study");
    await $(".grade-4").click();
    await $(".session-finished").waitForDisplayed();
    const after = await command("get_card", { cardId: before.card.id });
    assert.equal(after.schedule.reviewCount, 1);
    await $("button=Undo last review").click();
    await $(".study-card").waitForDisplayed();
    const undo = await command("get_card", { cardId: before.card.id });
    assert.equal(undo.schedule.reviewCount, 0);
    assert.equal(undo.schedule.phase, "new");
  });

  it("keeps a mistyped answer manually gradable and reverses a reversed card", async () => {
    await $('nav button[title="Add card"]').click();
    await $("button=Type in the Answer").click();
    await $('[aria-label="Front"][contenteditable="true"]').setValue(
      "What is the chemical symbol for water?",
    );
    await $('[aria-label="Back"][contenteditable="true"]').setValue("H₂O");
    await $("button*=Save card").click();
    await browser.waitUntil(
      async () => (await command("bootstrap")).decks[0].total === 2,
    );
    await $("button=Reversed").click();
    await $('[aria-label="Front"][contenteditable="true"]').setValue(
      "Thank you",
    );
    await $('[aria-label="Back"][contenteditable="true"]').setValue(
      "ありがとう",
    );
    await $("button*=Save card").click();
    await browser.waitUntil(
      async () => (await command("bootstrap")).decks[0].total === 3,
    );
    await $('nav button[title="Today"]').click();
    await $("button*=Start studying").click();
    await $(".study-card").waitForDisplayed();
    await $("button*=Reveal answer").click();
    await $(".grade-4").click();
    await $(".session-finished").waitForDisplayed();
    await $("button=Back to Today").click();
    await $("button*=Start studying").click();
    await $('[aria-label="Your answer"]').waitForDisplayed();
    await $('[aria-label="Your answer"]').setValue("water");
    await browser.keys("Enter");
    await $(".answer-comparison").waitForDisplayed();
    assert.match(await $(".answer-comparison").getText(), /water/);
    assert.equal(await $$(".grade-button").length, 4);
    await browser.saveScreenshot("artifacts/e2e/06-typed.png");
    await $(".grade-4").click();
    await $(".study-card").waitForDisplayed();
    await browser.waitUntil(async () =>
      (await $(".study-card").getText()).includes("ありがとう"),
    );
    await $("button*=Reveal answer").click();
    assert.match(await $(".study-card").getText(), /Thank you/);
    await $(".grade-4").click();
    await $(".session-finished").waitForDisplayed();
  });

  it("browses, searches, moves, suspends, buries, and restores a saved card", async () => {
    const defaults = (await command("bootstrap")).preferences.defaults;
    const deck = await command("save_deck", {
      id: null,
      name: "Japanese",
      color: "rose",
      cover: null,
      settings: defaults,
    });
    await browser.refresh();
    await $("h1").waitForDisplayed();
    await $('nav button[title="Browse"]').click();
    const search = $('input[aria-label="Search cards"]');
    await search.setValue("Thank you");
    await browser.waitUntil(async () => (await $$(".table-row").length) === 1);
    await $('.table-row input[type="checkbox"]').click();
    await $("button=Move").click();
    await selectOption('[role="dialog"] select', deck);
    await $('[role="dialog"]').$("button*=Move").click();
    await browser.waitUntil(
      async () => !(await $('[role="dialog"]').isExisting()),
    );
    assert.equal(
      (await command("bootstrap")).decks.find((item: any) => item.id === deck)
        .total,
      1,
    );
    await $('.table-row input[type="checkbox"]').click();
    await browser.execute(() =>
      (document.activeElement as HTMLElement)?.blur(),
    );
    await browser.keys("s");
    await browser.waitUntil(async () =>
      (await $(".table-row").getText()).toLowerCase().includes("suspended"),
    );
    await browser.keys(["Meta", "z"]);
    await browser.waitUntil(
      async () =>
        !(await $(".table-row").getText()).toLowerCase().includes("suspended"),
    );
    await $('.table-row input[type="checkbox"]').click();
    await browser.execute(() =>
      (document.activeElement as HTMLElement)?.blur(),
    );
    await browser.keys("b");
    await browser.waitUntil(async () =>
      (await $(".table-row").getText()).toLowerCase().includes("buried"),
    );
    await browser.keys(["Meta", "z"]);
    await browser.waitUntil(
      async () =>
        !(await $(".table-row").getText()).toLowerCase().includes("buried"),
    );
    await $('.table-row input[type="checkbox"]').click();
    await $('[aria-label="Delete selected notes"]').click();
    await $('[role="dialog"]').$("button=Move to Recently Deleted").click();
    await browser.waitUntil(async () => (await $$(".table-row").length) === 0);
    await browser.keys(["Meta", "z"]);
    await browser.waitUntil(async () => (await $$(".table-row").length) === 1);
    await search.setValue("");
    await browser.waitUntil(async () => (await $$(".table-row").length) === 3);
    await browser.saveScreenshot("artifacts/e2e/07-browser.png");
  });

  it("renders local images and equations, and shows artwork decks and actual statistics", async () => {
    const data = await command("bootstrap");
    const file = await command("test_grant", {
      path: path.resolve("public/tala-mark.png"),
      purpose: "image",
    });
    const media = await command("attach_file", { token: file.token });
    for (const [name, color] of [
      ["Astronomy", "blue"],
      ["World history", "amber"],
      ["Computer science", "violet"],
      ["Big ideas", "teal"],
    ]) {
      await command("save_deck", {
        id: null,
        name,
        color,
        cover: name === "Astronomy" ? media : null,
        settings: data.preferences.defaults,
      });
    }
    const decks = (await command("bootstrap")).decks;
    const astronomy = decks.find((deck: any) => deck.name === "Astronomy");
    const doc = (text: string) => ({
      type: "doc",
      content: [{ type: "paragraph", content: [{ type: "text", text }] }],
    });
    const card = await command("save_note", {
      id: null,
      deckId: astronomy.id,
      behavior: "normal",
      tags: ["physics"],
      front: {
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [
              { type: "text", text: "What does this equation describe?" },
            ],
          },
          { type: "blockMath", attrs: { latex: "E = mc^2" } },
          { type: "image", attrs: { mediaId: media, alt: "Tala star" } },
        ],
      },
      back: doc("Mass–energy equivalence."),
    });
    // Reload the bundled frontend to verify that the collection survives an app UI restart.
    await browser.refresh();
    await $("h1").waitForDisplayed();
    await $('nav button[title="Decks"]').click();
    await browser.waitUntil(
      async () => (await $$('[role="tree"] > [role="treeitem"]')).length === 6,
    );
    await browser.saveScreenshot("artifacts/e2e/08-decks.png");
    await $('nav button[title="Browse"]').click();
    await $('input[aria-label="Search cards"]').setValue("equation");
    await browser.waitUntil(async () => (await $$(".table-row").length) === 1);
    await $(".table-row button").click();
    await $('[role="dialog"] .katex').waitForDisplayed();
    assert.equal(
      await browser.execute(() => {
        const image = document.querySelector(
          '[role="dialog"] .rich-document img',
        ) as HTMLImageElement;
        return image?.complete && image.naturalWidth > 0;
      }),
      true,
    );
    await browser.saveScreenshot("artifacts/e2e/09-math-image.png");
    await $('[aria-label="Close dialog"]').click();
    assert.equal(
      (await command("get_card", { cardId: card.id })).front.content[2].attrs
        .mediaId,
      media,
    );
    await $('nav button[title="Statistics"]').click();
    await $(".statistics-page").waitForDisplayed();
    await browser.waitUntil(
      async () => (await $$(".recharts-surface").length) >= 2,
    );
    await browser.saveScreenshot("artifacts/e2e/10-statistics.png");
    await resize(1024, 700);
    await browser.saveScreenshot("artifacts/e2e/11-statistics-small.png");
    assert.equal(
      await browser.execute(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
      true,
    );
    await resize(1380, 900);
  });

  it("imports and exports through native file grants, then backs up and restores the collection", async () => {
    const folder = process.env.TALA_TEST_DATA_DIR!;
    const input = path.join(folder, "input.csv");
    writeFileSync(
      input,
      "Front,Back,Tags\nImported question,Imported answer,imported\n",
    );
    await command("test_grant", { path: input, purpose: "test-pick:import" });
    // Supply the native picker with a test fixture; all parsing, transactions and file writes remain real.
    await $('nav button[title="Settings"]').click();
    await $("button=Data & backups").click();
    await $("button*=Import CSV").click();
    await $("button=Choose file").click();
    await $(".import-table tbody tr").waitForDisplayed();
    await $("button=Import cards").waitForEnabled();
    await $("button=Import cards").click();
    await browser.waitUntil(
      async () => !(await $('[role="dialog"]').isExisting()),
    );
    const output = path.join(folder, "native-export.tala");
    await command("test_grant", { path: output, purpose: "test-save:native" });
    await $("button*=Export collection").click();
    await $("button=Save export").click();
    await browser.waitUntil(async () => existsSync(output));
    await browser.waitUntil(
      async () => !(await $('[role="dialog"]').isExisting()),
    );
    assert.ok(readFileSync(output).length > 1000);
    const total = (await command("bootstrap")).decks.reduce(
      (count: number, deck: any) => count + deck.total,
      0,
    );
    await $("button=Back up now").click();
    await browser.waitUntil(async () => (await command("backups")).length > 0);
    const restoreDeck = (await command("bootstrap")).decks[0].id;
    await command("save_note", {
      id: null,
      deckId: restoreDeck,
      behavior: "normal",
      tags: [],
      front: {
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [{ type: "text", text: "Created after backup" }],
          },
        ],
      },
      back: {
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [{ type: "text", text: "This is removed by restore" }],
          },
        ],
      },
    });
    assert.equal(
      (await command("bootstrap")).decks.reduce(
        (count: number, deck: any) => count + deck.total,
        0,
      ),
      total + 1,
    );
    await $("button=Run integrity check").click();
    await $("h3=Your collection looks healthy").waitForDisplayed();
    await browser.saveScreenshot("artifacts/e2e/12-settings-backups.png");
    await $(".backup-list").$("button=Restore").click();
    await $('[role="dialog"]').$("button=Restore collection").click();
    await browser.waitUntil(
      async () => !(await $('[role="dialog"]').isExisting()),
    );
    await browser.waitUntil(
      async () =>
        (await command("bootstrap")).decks.reduce(
          (count: number, deck: any) => count + deck.total,
          0,
        ) === total,
    );
    assert.equal((await command("integrity")).healthy, true);
  });
  it("navigates nested decks, scopes browsing, and imports an audio Anki package beneath a parent", async () => {
    const initial = await command("bootstrap");
    const biology = initial.decks.find((deck: any) => deck.name === "Biology");
    assert.ok(biology);

    await $('nav button[title="Decks"]').click();
    await $(`[aria-label="Open ${biology.name}"]`).click();
    await $("button=Add subdeck").click();
    await $('[role="dialog"] input').setValue("Genetics");
    await $("button=Create deck").click();
    await $("h1=Genetics").waitForDisplayed();
    const decksAfterCreate = await command("bootstrap");
    const genetics = decksAfterCreate.decks.find(
      (deck: any) => deck.name === "Genetics" && deck.parentId === biology.id,
    );
    assert.ok(genetics);
    await $("button=Add card").click();
    await $('[aria-label="Front"][contenteditable="true"]').setValue(
      "Genetics only deck fixture",
    );
    await $('[aria-label="Back"][contenteditable="true"]').setValue(
      "A nested answer",
    );
    const addAnother = $("label*=Add another after saving").$("input");
    if (await addAnother.isSelected()) await addAnother.click();
    await $("button*=Save card").click();
    await $("h1=Genetics").waitForDisplayed();

    await $('nav button[title="Decks"]').click();
    await $('[aria-label="Search decks"]').setValue("Genetics");
    await $("[aria-label='Open Biology::Genetics']").waitForDisplayed();
    await browser.saveScreenshot("artifacts/e2e/15-hierarchy.png");
    await $("[aria-label='Open Biology::Genetics']").click();
    await $("h1=Genetics").waitForDisplayed();
    const breadcrumbs = $('[aria-label="Deck path"]');
    assert.match(await breadcrumbs.getText(), /Biology.*Genetics/);
    await breadcrumbs.$("button").click();
    await $("h1=Biology").waitForDisplayed();

    await $('nav button[title="Browse"]').click();
    await selectOption('[aria-label="Filter by deck"]', biology.id);
    const browseCount = async () => {
      const label = await $(".browser-actions .small.muted").getText();
      const match = label.match(/([\d,]+) cards/);
      assert.ok(match, `Unexpected browse count label: ${label}`);
      return Number(match[1].replaceAll(",", ""));
    };
    await browser.waitUntil(async () => (await browseCount()) > 0);
    const subtreeCount = await browseCount();
    await $("label*=Only this deck").$("input").click();
    await browser.waitUntil(async () => (await browseCount()) < subtreeCount);
    assert.ok((await browseCount()) >= 1);

    const folder = process.env.TALA_TEST_DATA_DIR!;
    const audio = makeWav();
    const packagePath = path.join(folder, "ui-audio.apkg");
    createAnkiFixture(packagePath, audio);
    await command("test_grant", {
      path: packagePath,
      purpose: "test-pick:import",
    });
    await $('nav button[title="Decks"]').click();
    await $("[aria-label='Open Biology']").waitForDisplayed();
    await browser.execute(() =>
      (
        document.querySelector(
          '[aria-label="Options for Biology"]',
        ) as HTMLElement
      ).focus(),
    );
    await browser.keys("Enter");
    await $('[role="menu"]').waitForDisplayed();
    await $('//*[@role="menuitem" and contains(.,"Import cards")]').click();
    await $("button=Choose file").click();
    await $("h3=Preview").waitForDisplayed();
    await browser.waitUntil(async () =>
      (await $(".import-preview").getText()).includes("1 audio files"),
    );
    assert.equal(
      await $("[role='dialog'] .form-grid select").getValue(),
      biology.id,
    );
    await browser.waitUntil(async () =>
      (await $(".anki-destinations").getText()).includes("UI package"),
    );
    assert.match(await $(".anki-destinations").getText(), /Audio lesson/);
    await browser.saveScreenshot("artifacts/e2e/16-anki-preview.png");
    await $("button=Import cards").click();
    await browser.waitUntil(
      async () => !(await $('[role="dialog"]').isExisting()),
    );
    const afterImport = await command("bootstrap");
    const packageDeck = afterImport.decks.find(
      (deck: any) => deck.name === "UI package" && deck.parentId === biology.id,
    );
    const audioLesson = afterImport.decks.find(
      (deck: any) =>
        deck.name === "Audio lesson" && deck.parentId === packageDeck?.id,
    );
    assert.ok(packageDeck && audioLesson);
    const imported = await command("browse", {
      search: "Anki UI import prompt",
      deck: audioLesson.id,
      onlyThisDeck: true,
      tag: null,
      state: null,
      leech: false,
      trash: false,
      sort: "created",
      descending: false,
      offset: 0,
      limit: 10,
    });
    assert.equal(imported.total, 1);
    const importedCard = await command("get_card", {
      cardId: imported.cards[0].id,
    });
    assert.match(JSON.stringify(importedCard.back), /"type":"audio"/);
    assert.equal((await command("integrity")).healthy, true);
  });

  it("attaches audio, restores editor deletion with undo, persists autoplay, and keeps study controls safe", async () => {
    const folder = process.env.TALA_TEST_DATA_DIR!;
    const wavPath = path.join(folder, "e2e-tone.wav");
    writeFileSync(wavPath, makeWav());
    const mp3Path = extractReferenceMp3(folder);
    const audioPath = mp3Path ?? wavPath;
    const audioName = path.basename(audioPath);
    const defaults = (await command("bootstrap")).preferences.defaults;
    const deckId = await command("save_deck", {
      id: null,
      name: "E2E audio study",
      parentId: null,
      color: "teal",
      cover: null,
      settings: defaults,
    });
    await browser.refresh();
    await $("h1").waitForDisplayed();
    await $('nav button[title="Decks"]').click();
    await $(`[aria-label="Open E2E audio study"]`).click();
    await $("h1=E2E audio study").waitForDisplayed();
    await $("button=Add card").click();
    const front = $('[aria-label="Front"][contenteditable="true"]');
    await front.waitForDisplayed();
    await front.setValue("Audio study question");
    for (let clip = 0; clip < 2; clip++) {
      await command("test_grant", {
        path: audioPath,
        purpose: "test-pick:audio",
      });
      await $(
        '[aria-label="Front formatting"] [aria-label="Attach audio"]',
      ).click();
      await browser.waitUntil(
        async () => (await $$(".audio-clip")).length === clip + 1,
      );
    }
    await $("button*=Save card").waitForEnabled();
    await $('[aria-label^="Remove "]').click();
    await browser.waitUntil(async () => (await $$(".audio-clip")).length === 1);
    await $('[aria-label="Undo Front edit"]').click();
    await browser.waitUntil(async () => (await $$(".audio-clip")).length === 2);
    await $('[aria-label="Back"][contenteditable="true"]').setValue(
      "Audio study answer",
    );
    await $("button*=Save card").click();
    await browser.waitUntil(
      async () =>
        (await command("bootstrap")).decks.find(
          (deck: any) => deck.id === deckId,
        ).total === 1,
    );
    const audioQuery = await command("browse", {
      search: "Audio study question",
      deck: deckId,
      onlyThisDeck: true,
      tag: null,
      state: null,
      leech: false,
      trash: false,
      sort: "created",
      descending: false,
      offset: 0,
      limit: 10,
    });
    const audioCard = await command("get_card", {
      cardId: audioQuery.cards[0].id,
    });
    assert.equal(audioQuery.total, 1);
    const audioNodes = audioCard.front.content.filter(
      (node: any) => node.type === "audio",
    );
    assert.equal(audioNodes.length, 2);
    assert.ok(audioNodes.every((node: any) => node.attrs.mediaId));
    assert.match(JSON.stringify(audioCard.front), /Audio study question/);

    await $('nav button[title="Settings"]').click();
    const autoplay = $('[aria-label="Auto-play card audio"]');
    await autoplay.waitForDisplayed();
    if (await autoplay.isSelected()) await autoplay.click();
    await $("button=Save preferences").click();
    await browser.waitUntil(
      async () => !(await command("bootstrap")).preferences.audioAutoplay,
    );
    await autoplay.click();
    await $("button=Save preferences").click();
    await browser.waitUntil(
      async () => (await command("bootstrap")).preferences.audioAutoplay,
    );

    await $('nav button[title="Decks"]').click();
    await $(`[aria-label="Open E2E audio study"]`).click();
    await $("button*=Study").click();
    await $(".study-card").waitForDisplayed();
    const player = $(`audio[aria-label="${audioName}"]`);
    await player.waitForDisplayed();
    await browser.waitUntil(
      async () => (await player.getProperty("readyState")) >= 2,
    );
    await browser.execute((label) => {
      (window as any).__talaAudioPlayers = [
        ...document.querySelectorAll(`audio[aria-label="${label}"]`),
      ];
    }, audioName);
    await browser.waitUntil(
      async () => (await player.getProperty("paused")) === false,
    );
    const replayButtons = await $$(`[aria-label="Replay ${audioName}"]`);
    const audioPlayers = await $$(`audio[aria-label="${audioName}"]`);
    await replayButtons[replayButtons.length - 1].click();
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () =>
            (window as any).__talaAudioPlayers?.[0]?.paused &&
            !(window as any).__talaAudioPlayers?.[1]?.paused,
        ),
    );
    await browser.waitUntil(
      async () => (await audioPlayers[1].getProperty("currentTime")) > 0.1,
    );
    const duration = await audioPlayers[1].getProperty("duration");
    assert.ok(Number.isFinite(duration) && duration > 0);
    await browser.execute(() => window.dispatchEvent(new Event("blur")));
    await browser.waitUntil(
      async () =>
        await browser.execute(() =>
          (window as any).__talaAudioPlayers?.every(
            (audio: HTMLAudioElement) => audio.paused,
          ),
        ),
    );
    assert.equal(await $$(".grade-button").length, 0);
    assert.match(await $(".study-card").getText(), /QUESTION/);
    await browser.saveScreenshot("artifacts/e2e/17-audio-study.png");

    await replayButtons[replayButtons.length - 1].click();
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () => !(window as any).__talaAudioPlayers?.[1]?.paused,
        ),
    );
    await browser.waitUntil(
      async () => (await audioPlayers[1].getProperty("currentTime")) > 0.1,
    );
    await browser.execute(() =>
      (
        document.querySelector(
          '[aria-label="Study card options"]',
        ) as HTMLElement
      ).focus(),
    );
    await browser.keys("Enter");
    await $('[role="menu"]').waitForDisplayed();
    await $('//*[@role="menuitem" and contains(.,"Card information")]').click();
    await $('[role="dialog"]').waitForDisplayed();
    await browser.waitUntil(
      async () =>
        await browser.execute(() =>
          (window as any).__talaAudioPlayers?.every(
            (audio: HTMLAudioElement) => audio.paused,
          ),
        ),
    );
    await $('[aria-label="Close dialog"]').click();
    await browser.waitUntil(
      async () => !(await $('[role="dialog"]').isExisting()),
    );
    assert.equal(await $$(".grade-button").length, 0);
    assert.match(await $(".study-card").getText(), /QUESTION/);
    await replayButtons[replayButtons.length - 1].click();
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () => !(window as any).__talaAudioPlayers?.[1]?.paused,
        ),
    );
    await $("button*=Reveal answer").click();
    await $(".grade-button").waitForDisplayed();
    await browser.waitUntil(
      async () =>
        await browser.execute(() =>
          (window as any).__talaAudioPlayers?.every(
            (audio: HTMLAudioElement) => audio.paused,
          ),
        ),
    );
    assert.equal(await $$(".grade-button").length, 4);
    await $("button=Leave session").click();
    await $("h1").waitForDisplayed();
    assert.equal((await command("integrity")).healthy, true);
  });

  it("protects unsaved cards from navigation and native quit, and supports 125% scale at minimum size", async () => {
    await $('nav button[title="Add card"]').click();
    await $('[aria-label="Front"][contenteditable="true"]').setValue(
      "Unsaved draft",
    );
    await $('nav button[title="Decks"]').click();
    await $("h2=Leave without saving?").waitForDisplayed();
    await $('[role="dialog"]').$("button=Cancel").click();
    assert.match(
      await $('[aria-label="Front"][contenteditable="true"]').getText(),
      /Unsaved draft/,
    );
    await command("quit_app");
    await $("h2=Leave without saving?").waitForDisplayed();
    await $('[role="dialog"]').$("button=Cancel").click();
    await $('nav button[title="Settings"]').click();
    await $('[role="dialog"]').$("button=Discard changes").click();
    await selectOption('[aria-label="Interface scale"]', "125");
    await $("button=Save preferences").click();
    await browser.waitUntil(
      async () => (await command("bootstrap")).preferences.scale === 125,
    );
    await resize(900, 650);
    const frame = await browser.execute(() => {
      const e = document.querySelector(".app-shell") as HTMLElement;
      const r = e.getBoundingClientRect();
      const z = Number(getComputedStyle(e).zoom);
      return {
        width: r.width * z,
        height: r.height * z,
        viewportWidth: innerWidth,
        viewportHeight: innerHeight,
      };
    });
    assert.ok(
      Math.abs(frame.width - frame.viewportWidth) < 2 &&
        Math.abs(frame.height - frame.viewportHeight) < 2,
      `Scaled frame does not fill window: ${JSON.stringify(frame)}`,
    );
    for (const [page, selector] of [
      ["Decks", ".deck-tree"],
      ["Statistics", ".statistics-page"],
      ["Add card", ".editor-page"],
      ["Browse", ".browse-page"],
    ]) {
      await $(`nav button[title="${page}"]`).click();
      await $(selector).waitForDisplayed();
      const layout = await browser.execute(() => {
        const main = document.querySelector("main")!;
        return {
          width: main.clientWidth,
          scroll: main.scrollWidth,
          viewport: innerWidth,
          document: document.documentElement.scrollWidth,
        };
      });
      assert.ok(
        layout.scroll <= layout.width + 2 &&
          layout.document <= layout.viewport + 2,
        `${page} overflows: ${JSON.stringify(layout)}`,
      );
    }
    await browser.saveScreenshot("artifacts/e2e/13-minimum-scale.png");
    await $('nav button[title="Settings"]').click();
    await selectOption('[aria-label="Interface scale"]', "100");
    await $("button=Save preferences").click();
    await resize(1380, 900);
  });
  it("edits rich content and artwork through the interface without resetting review progress", async () => {
    await $('nav button[title="Browse"]').click();
    await $('input[aria-label="Search cards"]').setValue("powerhouse");
    await browser.waitUntil(async () => (await $$(".table-row").length) === 1);
    await $(".table-row button").click();
    await $("button=Edit card").click();
    const front = $('[aria-label="Front"][contenteditable="true"]');
    await front.waitForDisplayed();
    const before = await command("browse", {
      search: "powerhouse",
      deck: null,
      tag: null,
      state: null,
      leech: false,
      trash: false,
      sort: "created",
      descending: false,
      offset: 0,
      limit: 100,
    });
    await front.click();
    await browser.keys(["Meta", "a"]);
    await $('[aria-label="Front formatting"] [aria-label="Bold"]').click();
    await caretAtEnd('[aria-label="Front"][contenteditable="true"]');
    await $(
      '[aria-label="Front formatting"] [aria-label="Display equation"]',
    ).click();
    await $('[role="dialog"] input').setValue("E = mc^2");
    await $("button=Apply").click();
    await $(".editor-page .katex").waitForDisplayed();
    await caretAtEnd('[aria-label="Front"][contenteditable="true"]');
    await browser.execute((base64) => {
      const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
      const clipboard = new DataTransfer();
      clipboard.items.add(
        new File([bytes], "clipboard-star.png", { type: "image/png" }),
      );
      document
        .querySelector('[aria-label="Front"][contenteditable="true"]')!
        .dispatchEvent(
          new ClipboardEvent("paste", {
            clipboardData: clipboard,
            bubbles: true,
            cancelable: true,
          }),
        );
    }, readFileSync("src-tauri/icons/32x32.png").toString("base64"));
    await $(".editor-page .rich-document img").waitForDisplayed();
    await browser.saveScreenshot("artifacts/e2e/14-rich-edit.png");
    await browser.keys(["Meta", "s"]);
    await browser.waitUntil(
      async () => !(await $(".editor-page").isExisting()),
    );
    const edited = await command("get_card", { cardId: before.cards[0].id });
    assert.deepEqual(edited.schedule, before.cards[0].schedule);
    assert.match(JSON.stringify(edited.front), /blockMath/);
    assert.match(JSON.stringify(edited.front), /mediaId/);
    assert.match(JSON.stringify(edited.front), /bold/);
    await browser.execute(() =>
      (
        document.querySelector(
          '[aria-label="Options for Biology"]',
        ) as HTMLElement
      ).focus(),
    );
    await browser.keys("Enter");
    await $('[role="menu"]').waitForDisplayed();
    await $('//*[@role="menuitem" and contains(.,"Edit name")]').click();
    await command("test_grant", {
      path: path.resolve("src-tauri/icons/32x32.png"),
      purpose: "test-pick:image",
    });
    await $("button=Replace cover").click();
    await $("button=Save changes").click();
    await browser.waitUntil(
      async () => !(await $('[role="dialog"]').isExisting()),
    );
    const biology = (await command("bootstrap")).decks.find(
      (deck: any) => deck.name === "Biology",
    );
    assert.notEqual(biology.cover, null);
    await browser.execute(() =>
      (
        document.querySelector(
          '[aria-label="Options for Biology"]',
        ) as HTMLElement
      ).focus(),
    );
    await browser.keys("Enter");
    await $('[role="menu"]').waitForDisplayed();
    await $('//*[@role="menuitem" and contains(.,"Edit name")]').click();
    await $('[aria-label="Remove cover"]').click();
    await $("button=Save changes").click();
    await browser.waitUntil(
      async () =>
        (await command("bootstrap")).decks.find(
          (deck: any) => deck.name === "Biology",
        ).cover === null,
    );
    assert.equal((await command("integrity")).healthy, true);
  });
});
