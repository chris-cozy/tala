import assert from "node:assert/strict";
import { writeFileSync, readFileSync, existsSync } from "node:fs";
import path from "node:path";

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
    await browser.waitUntil(async () => (await $$(".deck-tile").length) === 6);
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
      ["Decks", ".deck-grid"],
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
