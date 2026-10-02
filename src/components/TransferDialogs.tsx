import { useEffect, useState } from "react";
import { Download, FileUp, ShieldCheck, ArrowRight } from "lucide-react";
import { AnkiImportDialog } from "./AnkiImportDialog";
import type { FileSelection } from "../bindings/FileSelection";
import type { Behavior } from "../bindings/Behavior";
import type { ImportConfig } from "../bindings/ImportConfig";
import type { ImportPreview } from "../bindings/ImportPreview";
import { useTala, Field } from "../lib/context";
import { errorMessage, pickFile, rpc, saveFile } from "../lib/api";
import { number } from "../lib/format";
import { Button, ErrorPanel, Modal } from "./ui";
export function ImportDialog({
  deckId,
  onClose,
}: {
  deckId?: string;
  onClose: () => void;
}) {
  const { data, run } = useTala();
  const [ankiFile, setAnkiFile] = useState<FileSelection | null>(null);
  const [fileName, setFileName] = useState("");
  const [busy, setBusy] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [error, setError] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [config, setConfig] = useState<ImportConfig>({
    pathToken: "",
    delimiter: "csv",
    hasHeader: true,
    frontColumn: 0,
    backColumn: 1,
    tagsColumn: null,
    deckId: deckId ?? data.decks[0]?.id ?? "",
    behavior: "normal",
    duplicates: "skip",
    previewDigest: null,
  });
  function update<K extends keyof ImportConfig>(
    key: K,
    value: ImportConfig[K],
  ) {
    setConfig((c) => ({ ...c, [key]: value }));
  }
  async function choose() {
    const file = await run(() => pickFile("import"));
    if (file) {
      if (file.name.toLowerCase().endsWith(".apkg")) {
        setAnkiFile(file);
        return;
      }
      setAnkiFile(null);
      setFileName(file.name);
      setConfig((c) => ({
        ...c,
        pathToken: file.token,
        delimiter: file.name.toLowerCase().endsWith(".tsv") ? "tsv" : "csv",
      }));
    }
  }
  useEffect(() => {
    if (ankiFile || !config.pathToken || !config.deckId) return;
    // Mapping changes can race a native preview; only the latest configuration may enable import.
    let cancelled = false;
    setPreviewing(true);
    setPreview(null);
    const timer = setTimeout(() => {
      rpc({ action: "preview_import", payload: config })
        .then((result) => {
          if (!cancelled) {
            setPreview(result);
            setError("");
          }
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
  }, [config, ankiFile]);
  async function commit() {
    if (!preview) return;
    setBusy(true);
    const result = await run(() =>
      rpc({
        action: "commit_import",
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
  if (ankiFile)
    return (
      <AnkiImportDialog
        key={ankiFile.token}
        file={ankiFile}
        parentId={deckId}
        onChoose={choose}
        onClose={onClose}
      />
    );
  const columns = preview?.headers.length
    ? preview.headers
    : ["Column 1", "Column 2", "Column 3"];
  return (
    <Modal
      title="Bring your knowledge along"
      description="Import Anki, CSV, or TSV files. Preview and validate everything before your collection changes."
      open
      onClose={onClose}
      wide
    >
      <div className="transfer-file">
        <FileUp size={27} />
        <div>
          <strong>{fileName || "Choose a flashcard file"}</strong>
          <small>Anki .apkg · CSV or TSV text</small>
        </div>
        <Button onClick={choose}>
          {fileName ? "Change file" : "Choose file"}
        </Button>
      </div>
      {fileName && !data.decks.length && (
        <p className="notice">
          Create a Tala deck to receive CSV/TSV cards. Anki packages create
          their own decks.
        </p>
      )}
      {fileName && (
        <>
          <div className="form-grid three">
            <Field label="Format">
              <select
                value={config.delimiter}
                onChange={(e) => update("delimiter", e.target.value)}
              >
                <option value="csv">CSV (comma)</option>
                <option value="tsv">TSV (tab)</option>
              </select>
            </Field>
            <Field label="Deck">
              <select
                value={config.deckId}
                onChange={(e) => update("deckId", e.target.value)}
              >
                {data.decks.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.path}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Card behavior">
              <select
                value={config.behavior}
                onChange={(e) => update("behavior", e.target.value as Behavior)}
              >
                <option value="normal">Normal</option>
                <option value="reversed">Reversed</option>
                <option value="typed">Type in the Answer</option>
              </select>
            </Field>
          </div>
          <label className="check-label">
            <input
              type="checkbox"
              checked={config.hasHeader}
              onChange={(e) => update("hasHeader", e.target.checked)}
            />
            First row contains column names
          </label>
          <div className="form-grid three">
            <Field label="Front column">
              <select
                value={config.frontColumn}
                onChange={(e) => update("frontColumn", Number(e.target.value))}
              >
                {columns.map((label, i) => (
                  <option value={i} key={i}>
                    {label || `Column ${i + 1}`}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Back column">
              <select
                value={config.backColumn}
                onChange={(e) => update("backColumn", Number(e.target.value))}
              >
                {columns.map((label, i) => (
                  <option value={i} key={i}>
                    {label || `Column ${i + 1}`}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Tags column" hint="Separate tags with semicolons.">
              <select
                value={config.tagsColumn ?? ""}
                onChange={(e) =>
                  update(
                    "tagsColumn",
                    e.target.value === "" ? null : Number(e.target.value),
                  )
                }
              >
                <option value="">No tags</option>
                {columns.map((label, i) => (
                  <option value={i} key={i}>
                    {label || `Column ${i + 1}`}
                  </option>
                ))}
              </select>
            </Field>
          </div>
          <Field label="When the Front and behavior already match a note in this deck">
            <select
              value={config.duplicates}
              onChange={(e) => update("duplicates", e.target.value)}
            >
              <option value="skip">Skip duplicates</option>
              <option value="update">
                Update existing notes · preserve scheduling
              </option>
              <option value="separate">Import as separate notes</option>
            </select>
          </Field>
          {error && <ErrorPanel message={error} />}
          <div className="import-preview">
            <div className="section-heading">
              <h3>{previewing ? "Validating preview…" : "Preview"}</h3>
              {preview && (
                <small>
                  {number(preview.total)} rows · {number(preview.duplicates)}{" "}
                  duplicates
                </small>
              )}
            </div>
            {preview?.errors.length ? (
              <div className="error-panel">
                <strong>Resolve these issues before importing</strong>
                <ul>
                  {preview.errors.slice(0, 5).map((message, i) => (
                    <li key={i}>{message}</li>
                  ))}
                </ul>
              </div>
            ) : null}
            {preview && (
              <div className="import-table">
                <table>
                  <thead>
                    <tr>
                      <th>Row</th>
                      <th>Front</th>
                      <th>Back</th>
                      <th>Tags</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.rows.map((row) => (
                      <tr
                        key={row.line}
                        className={row.error ? "invalid-row" : ""}
                      >
                        <td>
                          {row.line}
                          {row.duplicate && <small>Duplicate</small>}
                        </td>
                        <td>{row.front}</td>
                        <td>{row.back}</td>
                        <td>{row.tags.join(", ")}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}{" "}
            {preview && preview.total > 50 && (
              <small className="muted">
                Showing the first 50 rows. All {number(preview.total)} rows have
                been checked.
              </small>
            )}
          </div>
        </>
      )}
      <div className="modal-actions">
        <Button onClick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          disabled={!preview || !!preview.errors.length || previewing}
          busy={busy}
          onClick={commit}
        >
          Import cards
          <ArrowRight size={16} />
        </Button>
      </div>
    </Modal>
  );
}
export function ExportDialog({
  deckId,
  ids,
  onClose,
}: {
  deckId?: string;
  ids?: string[];
  onClose: () => void;
}) {
  const { data, run } = useTala();
  const [format, setFormat] = useState(
    deckId || ids?.length ? "csv" : "native",
  );
  const [busy, setBusy] = useState(false);
  const [onlyThisDeck, setOnlyThisDeck] = useState(false);
  const name = data.decks.find((d) => d.id === deckId)?.name;
  async function save() {
    setBusy(true);
    const result = await run(async () => {
      const file = await saveFile(
        format === "native" ? "native" : "delimited",
        format === "native"
          ? "Tala collection.tala"
          : `${name ?? "Tala cards"}.${format}`,
      );
      if (!file) return false;
      if (format === "native")
        await rpc({ action: "export_native", payload: { token: file.token } });
      else
        await rpc({
          action: "export_delimited",
          payload: {
            token: file.token,
            format,
            deckId: deckId ?? null,
            ids: ids ?? [],
            onlyThisDeck,
          },
        });
      return true;
    });
    setBusy(false);
    if (result) {
      await run(() => Promise.resolve(), "Export saved");
      onClose();
    }
  }
  return (
    <Modal
      title="Take your knowledge with you"
      description={
        ids?.length
          ? `${ids.length} selected notes`
          : name
            ? `Export ${name}`
            : "Export your Tala collection"
      }
      open
      onClose={onClose}
    >
      <div className="export-choices">
        {[
          {
            value: "native",
            title: "Complete Tala collection",
            description:
              "All decks, formatting, media, schedules, and review history.",
            icon: ShieldCheck,
          },
          {
            value: "csv",
            title: "CSV",
            description: "Plain text Front, Back, behavior, deck, and tags.",
            icon: Download,
          },
          {
            value: "tsv",
            title: "TSV",
            description: "The same fields, separated with tabs.",
            icon: Download,
          },
        ].map((choice) => (
          <label
            key={choice.value}
            className={`export-choice ${format === choice.value ? "selected" : ""}`}
          >
            <input
              type="radio"
              name="export-format"
              value={choice.value}
              checked={format === choice.value}
              onChange={() => setFormat(choice.value)}
            />
            <choice.icon size={20} />
            <span>
              <strong>{choice.title}</strong>
              <small>{choice.description}</small>
            </span>
          </label>
        ))}
      </div>
      {deckId && !ids?.length && format !== "native" && (
        <label className="checkbox-line">
          <input
            type="checkbox"
            checked={onlyThisDeck}
            onChange={(event) => setOnlyThisDeck(event.target.checked)}
          />
          Only this deck (exclude subdecks)
        </label>
      )}
      <p className="notice">
        {format === "native"
          ? "Native exports contain the entire collection, even when opened from a deck or selection. Restore them from Settings → Data & backups."
          : "CSV/TSV exports do not preserve rich formatting, images, audio, or scheduling. Choose Tala format for a complete, recoverable copy. Spreadsheet apps may interpret text beginning with “=” as formulas."}
      </p>
      <div className="modal-actions">
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" busy={busy} onClick={save}>
          <Download size={16} />
          Save export
        </Button>
      </div>
    </Modal>
  );
}
