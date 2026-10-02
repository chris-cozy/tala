import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AnkiImportPreview } from "../bindings/AnkiImportPreview";
import type { FileSelection } from "../bindings/FileSelection";
import { rpc } from "../lib/api";
import { TalaContext, type TalaContextValue } from "../lib/context";

vi.mock("../lib/api", () => ({
  rpc: vi.fn(),
  errorMessage: (error: unknown) => String(error),
}));

import { AnkiImportDialog } from "./AnkiImportDialog";

const file: FileSelection = { token: "package-token", name: "lesson.apkg" };

function preview(
  overrides: Partial<AnkiImportPreview> = {},
): AnkiImportPreview {
  return {
    digest: "package-digest",
    total: 3,
    duplicates: 0,
    affected: 1,
    audioFiles: 2,
    audioAssets: 1,
    imageFiles: 0,
    decks: [{ path: "Language::Basics", cards: 3, matches: [] }],
    cards: [],
    warnings: [],
    issues: ["One unsupported card"],
    blockingErrors: [],
    ...overrides,
  };
}

function renderDialog() {
  const onClose = vi.fn();
  const onChoose = vi.fn(async () => {});
  const run = vi.fn(async (operation: () => Promise<unknown>) => operation());
  const value = {
    data: { decks: [], mediaDir: "/isolated/media" },
    run,
  } as unknown as TalaContextValue;
  render(
    <TalaContext.Provider value={value}>
      <AnkiImportDialog file={file} onChoose={onChoose} onClose={onClose} />
    </TalaContext.Provider>,
  );
  return { onClose, onChoose, run };
}

describe("Anki import safety gates", () => {
  beforeEach(() => vi.clearAllMocks());
  afterEach(cleanup);

  it("requires an explicit affected-card skip and commits the reviewed digest", async () => {
    const result = preview();
    vi.mocked(rpc).mockImplementation(async (request) => {
      if (request.action === "preview_anki") return result as never;
      return { imported: 2, updated: 0, skipped: 1 } as never;
    });
    const { onClose } = renderDialog();
    const importButton = await screen.findByRole("button", {
      name: /Import cards/,
    });
    expect(importButton).toBeDisabled();
    await screen.findByText("1 card needs attention");

    fireEvent.click(
      await screen.findByRole("checkbox", {
        name: /Skip these affected cards and import the supported remainder/,
      }),
    );
    await waitFor(() => expect(importButton).toBeEnabled());
    fireEvent.click(importButton);
    await waitFor(() => expect(onClose).toHaveBeenCalledOnce());
    expect(rpc).toHaveBeenCalledWith({
      action: "commit_anki",
      payload: expect.objectContaining({
        pathToken: file.token,
        skipAffected: true,
        previewDigest: "package-digest",
      }),
    });
  });

  it("keeps import disabled while a destination choice is blocking", async () => {
    vi.mocked(rpc).mockResolvedValue(
      preview({
        affected: 0,
        issues: [],
        blockingErrors: ["Choose which existing deck to use."],
      }) as never,
    );
    renderDialog();
    const importButton = await screen.findByRole("button", {
      name: /Import cards/,
    });
    expect(
      await screen.findByText("Choose which existing deck to use."),
    ).toBeInTheDocument();
    expect(importButton).toBeDisabled();
    expect(rpc).not.toHaveBeenCalledWith(
      expect.objectContaining({ action: "commit_anki" }),
    );
  });
});
