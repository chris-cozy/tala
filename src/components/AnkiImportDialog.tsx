import { useEffect, useState } from "react";
import { ArrowRight, FileUp } from "lucide-react";
import type { FileSelection } from "../bindings/FileSelection";
import type { AnkiImportConfig } from "../bindings/AnkiImportConfig";
import type { AnkiImportPreview } from "../bindings/AnkiImportPreview";
import { Field, useTala } from "../lib/context";
import { errorMessage, rpc } from "../lib/api";
import { number } from "../lib/format";
import { Button, ErrorPanel, Modal } from "./ui";
import { ContentRender, type Doc } from "./RichContent";

/** Package previews contain converted documents, but assets remain in native staging. */
export function packagePreviewDoc(value: unknown): Doc {
  const doc = JSON.parse(JSON.stringify(value)) as Doc;
  function walk(node: Doc): Doc {
    if (node.type === "audio" || node.type === "image")
      return {
        type: "paragraph",
        content: [
          {
            type: "text",
            text:
              node.type === "audio"
                ? `Audio: ${node.attrs?.label || "clip"}`
                : "Local image",
          },
        ],
      };
    return {
      ...node,
      ...(node.content ? { content: node.content.map(walk) } : {}),
    };
  }
  return walk(doc);
}

export function AnkiImportDialog({
  file,
  parentId,
  onChoose,
  onClose,
}: {
  file: FileSelection;
  parentId?: string;
  onChoose: () => Promise<void>;
  onClose: () => void;
}) {
  const { data, run } = useTala();
  const [config, setConfig] = useState<AnkiImportConfig>({
    pathToken: file.token,
    parentId: parentId ?? null,
    duplicates: "skip",
    skipAffected: false,
    destinations: {},
    previewDigest: null,
  });
  const [preview, setPreview] = useState<AnkiImportPreview | null>(null);
  const [error, setError] = useState("");
  const [previewing, setPreviewing] = useState(true);
  const [busy, setBusy] = useState(false);
  const [issueCount, setIssueCount] = useState(20);
  useEffect(() => {
    let cancelled = false;
    setPreviewing(true);
    setPreview(null);
    setError("");
    const timer = setTimeout(() => {
      rpc({ action: "preview_anki", payload: config })
        .then((result) => {
          if (!cancelled) setPreview(result);
        })
        .catch((error) => {
          if (!cancelled) setError(errorMessage(error));
        })
        .finally(() => {
          if (!cancelled) setPreviewing(false);
        });
    }, 200);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [config]);
  function chooseDestination(path: string, id: string) {
    const destinations = Object.fromEntries(
      Object.entries(config.destinations).filter(
        ([key]) => key !== path && !key.startsWith(`${path}::`),
      ),
    );
    if (id) destinations[path] = id;
    setConfig({ ...config, destinations });
  }
  async function commit() {
    if (!preview) return;
    setBusy(true);
    const result = await run(() =>
      rpc({
        action: "commit_anki",
        payload: { ...config, previewDigest: preview.digest },
      }),
    );
    setBusy(false);
    if (result) {
      await run(
        () => Promise.resolve(),
        `${number(result.imported)} imported · ${number(result.updated)} updated · ${number(result.skipped)} skipped`,
      );
      onClose();
    }
  }
  return (
    <Modal
      title="Bring your Anki decks along"
      description="Preview editable Tala cards, nested decks, and bundled audio before importing."
      open
      wide
      onClose={onClose}
    >
      <div className="transfer-file">
        <FileUp size={27} />
        <div>
          <strong>{file.name}</strong>
          <small>Anki package · up to 1 GB</small>
        </div>
        <Button onClick={() => void onChoose()} disabled={busy}>
          Change file
        </Button>
      </div>
      <div className="form-grid">
        <Field label="Place imported decks beneath">
          <select
            disabled={busy}
            value={config.parentId ?? ""}
            onChange={(e) =>
              setConfig({
                ...config,
                parentId: e.target.value || null,
                destinations: {},
              })
            }
          >
            <option value="">Top level</option>
            {data.decks.map((deck) => (
              <option key={deck.id} value={deck.id}>
                {deck.path}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Previously imported cards">
          <select
            disabled={busy}
            value={config.duplicates}
            onChange={(e) =>
              setConfig({ ...config, duplicates: e.target.value })
            }
          >
            <option value="skip">Skip matches</option>
            <option value="update">Update · preserve Tala scheduling</option>
            <option value="separate">Import separately</option>
          </select>
        </Field>
      </div>
      <p className="notice">
        New cards start as New with Tala’s default settings. Templates become
        editable text and local media; custom scripts and styling may be
        omitted.
      </p>
      {error && <ErrorPanel message={error} />}
      {Object.keys(config.destinations).length > 0 && (
        <Button
          disabled={busy}
          onClick={() => setConfig({ ...config, destinations: {} })}
        >
          Reset destination choices
        </Button>
      )}
      <div className="import-preview">
        <div className="section-heading">
          <h3>{previewing ? "Checking package and media…" : "Preview"}</h3>
          {preview && (
            <small>
              {number(preview.total)} cards · {number(preview.duplicates)}{" "}
              matches · {number(preview.audioFiles)} audio files ·{" "}
              {number(preview.imageFiles)} images
            </small>
          )}
        </div>
        {preview && (
          <>
            {preview.audioFiles !== preview.audioAssets && (
              <p className="notice">
                Identical audio files share storage:{" "}
                {number(preview.audioAssets)} audio assets preserve all{" "}
                {number(preview.audioFiles)} referenced filenames.
              </p>
            )}
            <h4>Destination decks · {number(preview.decks.length)}</h4>
            <div className="anki-destinations">
              {preview.decks.map((deck) => (
                <div
                  key={deck.path}
                  style={{
                    paddingLeft: `${Math.min(6, deck.path.split("::").length - 1) * 16}px`,
                  }}
                >
                  <strong>{deck.path.split("::").at(-1)}</strong>{" "}
                  <small className="muted">
                    {number(deck.cards)} direct cards ·{" "}
                    {deck.matches.length ? "existing" : "new"}
                  </small>
                  {deck.matches.length > 1 && (
                    <Field label={`Existing destination for ${deck.path}`}>
                      <select
                        value={config.destinations[deck.path] ?? ""}
                        onChange={(e) =>
                          chooseDestination(deck.path, e.target.value)
                        }
                      >
                        <option value="">Choose a matching deck</option>
                        {deck.matches.map((id) => (
                          <option key={id} value={id}>
                            {data.decks.find((deck) => deck.id === id)?.path} ·{" "}
                            {id.slice(0, 8)}
                          </option>
                        ))}
                      </select>
                    </Field>
                  )}
                </div>
              ))}
            </div>
            {preview.warnings.length > 0 && (
              <div className="notice">
                <strong>Conversion notes</strong>
                <ul>
                  {preview.warnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              </div>
            )}
            {preview.blockingErrors.length > 0 && (
              <div className="error-panel">
                <ul>
                  {preview.blockingErrors.map((issue) => (
                    <li key={issue}>{issue}</li>
                  ))}
                </ul>
              </div>
            )}
            {preview.affected > 0 && (
              <>
                <div className="error-panel anki-issues">
                  <strong>
                    {number(preview.affected)}{" "}
                    {preview.affected === 1 ? "card needs" : "cards need"}{" "}
                    attention
                  </strong>
                  <ul>
                    {preview.issues.slice(0, issueCount).map((issue, i) => (
                      <li key={i}>{issue}</li>
                    ))}
                  </ul>
                  {preview.issues.length > issueCount && (
                    <Button
                      onClick={() => setIssueCount((count) => count + 100)}
                    >
                      Show more issues
                    </Button>
                  )}
                </div>
                <label className="check-label">
                  <input
                    type="checkbox"
                    checked={config.skipAffected}
                    onChange={(e) =>
                      setConfig({ ...config, skipAffected: e.target.checked })
                    }
                  />
                  Skip these affected cards and import the supported remainder
                </label>
              </>
            )}
            <details>
              <summary>
                Converted card samples · first {preview.cards.length}
              </summary>
              {preview.cards.map((card) => (
                <article className="anki-card-preview" key={card.number}>
                  <small className="muted">
                    Card {card.number} · {card.deckPath}
                    {card.duplicate ? " · Already imported" : ""}
                  </small>
                  {card.error ? (
                    <p className="audio-error">{card.error}</p>
                  ) : (
                    <>
                      <p className="eyebrow">QUESTION</p>
                      <ContentRender
                        value={packagePreviewDoc(card.front)}
                        directory={data.mediaDir}
                      />
                      <p className="eyebrow">ANSWER</p>
                      <ContentRender
                        value={packagePreviewDoc(card.back)}
                        directory={data.mediaDir}
                      />
                    </>
                  )}
                </article>
              ))}
            </details>
          </>
        )}
      </div>
      <div className="modal-actions">
        <Button onClick={onClose} disabled={busy}>
          Cancel
        </Button>
        <Button
          variant="primary"
          busy={busy}
          disabled={
            !preview ||
            previewing ||
            !!preview.blockingErrors.length ||
            preview.affected === preview.total ||
            (!!preview.affected && !config.skipAffected)
          }
          onClick={() => void commit()}
        >
          Import cards
          <ArrowRight size={16} />
        </Button>
      </div>
    </Modal>
  );
}
