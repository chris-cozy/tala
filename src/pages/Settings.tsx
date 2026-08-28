import { version } from "../../package.json";
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Archive,
  CheckCircle2,
  Download,
  FolderOpen,
  HardDrive,
  RotateCcw,
  ShieldCheck,
  Upload,
  Keyboard,
  Moon,
} from "lucide-react";
import type { IntegrityReport } from "../bindings/IntegrityReport";
import { useTala, Field } from "../lib/context";
import { errorMessage, pickFile, rpc, saveFile } from "../lib/api";
import { dateTime, number } from "../lib/format";
import { Button, ErrorPanel, PageHeader } from "../components/ui";
import { SchedulingForm } from "../components/SchedulingSettings";
export default function SettingsPage() {
  const { data, run, confirm, importCards, exportCards } = useTala();
  const [tab, setTab] = useState("general");
  const [preferences, setPreferences] = useState(data.preferences);
  const [valid, setValid] = useState(true);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<IntegrityReport | null>(null);
  const backups = useQuery({
    queryKey: ["backups"],
    queryFn: () => rpc({ action: "backups" }),
    enabled: tab === "data",
    refetchInterval: 30000,
  });
  async function save() {
    setBusy(true);
    await run(
      () => rpc({ action: "save_preferences", payload: preferences }),
      "Preferences saved",
    );
    setBusy(false);
  }
  async function restore(name?: string) {
    const file = name ? null : await run(() => pickFile("restore"));
    if (!name && !file) return;
    if (
      !(await confirm({
        title: "Replace this collection?",
        message: `Restore “${name ?? file?.name}”? Your current decks, notes, schedules, and history will be replaced. Tala will first make a complete safety backup.`,
        confirm: "Restore collection",
        danger: true,
      }))
    )
      return;
    setBusy(true);
    const result = await run(
      () =>
        name
          ? rpc({ action: "restore_backup", payload: { name } })
          : rpc({ action: "restore_file", payload: { token: file!.token } }),
      "Collection restored",
    );
    setBusy(false);
    if (result !== undefined) {
      setReport(null);
      const restored = await rpc({ action: "bootstrap" });
      setPreferences(restored.preferences);
    }
  }
  async function integrity() {
    setBusy(true);
    const result = await run(() => rpc({ action: "integrity" }));
    setBusy(false);
    if (result) setReport(result);
  }
  async function cleanup() {
    if (
      await confirm({
        title: "Remove unused images?",
        message:
          "Only media not referenced by any note (including Recently Deleted) or deck will be removed. Existing backups retain their own copies.",
        confirm: "Remove unused media",
        danger: true,
      })
    ) {
      await run(() => rpc({ action: "cleanup_media" }), "Unused media removed");
      await integrity();
    }
  }
  async function repair() {
    if (
      !(await confirm({
        title: "Rebuild derived indexes?",
        message:
          "Tala will first create a safety backup, then rebuild search text, the search index, and media references from saved notes. Card content, schedules, and review history will not change.",
        confirm: "Back up and rebuild",
      }))
    )
      return;
    setBusy(true);
    const result = await run(
      () => rpc({ action: "repair_indexes" }),
      "Derived indexes rebuilt",
    );
    setBusy(false);
    if (result) setReport(result);
  }
  async function diagnostics() {
    await run(async () => {
      const file = await saveFile("diagnostics", "Tala diagnostics.json");
      if (file)
        await rpc({
          action: "export_diagnostics",
          payload: { token: file.token },
        });
    });
  }
  return (
    <div className="page settings-page">
      <PageHeader
        eyebrow="A SPACE THAT WORKS FOR YOU"
        title="Settings"
        subtitle="Your practice, your pace. Everything stays on this device."
      />
      <div className="settings-tabs">
        {[
          ["general", "Appearance & shortcuts"],
          ["study", "Study defaults"],
          ["data", "Data & backups"],
        ].map(([value, label]) => (
          <button
            key={value}
            className={tab === value ? "selected" : ""}
            onClick={() => setTab(value)}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === "general" ? (
        <>
          <section className="panel settings-panel">
            <div className="panel-heading">
              <div>
                <h2>Quiet by design</h2>
                <p>A focused, dark workspace with violet accents.</p>
              </div>
              <Moon size={20} className="text-violet" />
            </div>
            <div className="settings-row">
              <div>
                <strong>Interface scale</strong>
                <p>Make the interface comfortable for your screen.</p>
              </div>
              <select
                aria-label="Interface scale"
                value={preferences.scale}
                onChange={(e) =>
                  setPreferences({
                    ...preferences,
                    scale: Number(e.target.value),
                  })
                }
              >
                {[90, 100, 110, 125].map((scale) => (
                  <option key={scale} value={scale}>
                    {scale}%
                  </option>
                ))}
              </select>
            </div>
            <div className="settings-row">
              <div>
                <strong>Motion</strong>
                <p>Tala respects your system’s Reduce Motion preference.</p>
              </div>
              <span className="muted small">System controlled</span>
            </div>
            <div className="settings-row">
              <div>
                <strong>Study-day boundary</strong>
                <p>
                  Limits and buried cards reset at local midnight. Historical
                  study days remain recorded.
                </p>
              </div>
              <span className="muted small">12:00 AM</span>
            </div>
          </section>
          <section className="panel settings-panel">
            <div className="panel-heading">
              <div>
                <h2>Stay in the flow</h2>
                <p>A complete study session, without reaching for the mouse.</p>
              </div>
              <Keyboard size={20} />
            </div>
            <div className="shortcut-grid">
              {[
                ["Reveal answer", "Space"],
                ["Submit typed answer", "Enter"],
                ["Again / Hard / Good / Easy", "1 / 2 / 3 / 4"],
                ["Undo review or bulk action", "⌘ Z"],
                ["Add card", "⌘ N"],
                ["Open browser", "⌘ B"],
                ["Search current screen", "⌘ F"],
                ["Save card", "⌘ S"],
                ["Edit studying card", "E"],
                ["Bury / Suspend studying card", "B / S"],
              ].map(([label, keys]) => (
                <div key={label}>
                  <span>{label}</span>
                  <kbd>{keys}</kbd>
                </div>
              ))}
            </div>
          </section>
          <div className="settings-save">
            <Button variant="primary" busy={busy} onClick={save}>
              Save preferences
            </Button>
          </div>
        </>
      ) : tab === "study" ? (
        <>
          <section className="panel settings-panel">
            <div className="notice">
              These defaults are copied into newly created decks. Existing decks
              keep their own settings.
            </div>
            <SchedulingForm
              value={preferences.defaults}
              onChange={(defaults) =>
                setPreferences({ ...preferences, defaults })
              }
              onValidity={setValid}
            />
          </section>
          <div className="settings-save">
            <Button
              variant="primary"
              busy={busy}
              disabled={!valid}
              onClick={save}
            >
              Save defaults
            </Button>
          </div>
        </>
      ) : (
        <>
          <section className="panel settings-panel">
            <div className="panel-heading">
              <div>
                <h2>Your collection belongs to you</h2>
                <p>
                  Transfer notes or create a complete copy, including media and
                  schedules.
                </p>
              </div>
              <HardDrive size={21} className="text-violet" />
            </div>
            <div className="data-actions">
              <Button onClick={() => importCards()}>
                <Upload size={16} />
                Import CSV / TSV
              </Button>
              <Button onClick={() => exportCards()}>
                <Download size={16} />
                Export collection
              </Button>
              <Button onClick={() => void restore()} busy={busy}>
                <RotateCcw size={16} />
                Restore .tala file
              </Button>
            </div>
            <div className="data-location">
              <span>Collection location</span>
              <code>{data.dataDir}</code>
              <Button
                variant="ghost"
                onClick={() =>
                  void run(() => rpc({ action: "open_data_directory" }))
                }
              >
                <FolderOpen size={16} />
                Open folder
              </Button>
            </div>
          </section>
          <section className="panel settings-panel">
            <div className="panel-heading">
              <div>
                <h2>A little peace of mind</h2>
                <p>
                  Automatic backups run daily after changes. Manual and safety
                  backups are retained separately.
                </p>
              </div>
              <ShieldCheck size={21} className="text-teal" />
            </div>
            <div className="form-grid">
              <label className="check-label">
                <input
                  type="checkbox"
                  checked={preferences.backupEnabled}
                  onChange={(e) =>
                    setPreferences({
                      ...preferences,
                      backupEnabled: e.target.checked,
                    })
                  }
                />
                <span>
                  Automatic local backups
                  <small>Complete database and media snapshots.</small>
                </span>
              </label>
              <Field label="Automatic backups to keep">
                <input
                  type="number"
                  min={1}
                  max={100}
                  value={preferences.backupRetention}
                  onChange={(e) =>
                    setPreferences({
                      ...preferences,
                      backupRetention: Number(e.target.value),
                    })
                  }
                />
              </Field>
            </div>
            <div className="inline">
              <Button variant="primary" busy={busy} onClick={save}>
                Save backup settings
              </Button>
              <Button
                onClick={async () => {
                  setBusy(true);
                  await run(
                    () => rpc({ action: "create_backup" }),
                    "Complete backup created",
                  );
                  setBusy(false);
                }}
                busy={busy}
              >
                <Archive size={16} />
                Back up now
              </Button>
            </div>
            {data.backupWarning && <ErrorPanel message={data.backupWarning} />}
            <div className="backup-list">
              {backups.error ? (
                <ErrorPanel message={errorMessage(backups.error)} />
              ) : backups.data?.length ? (
                backups.data.map((backup) => (
                  <div key={backup.name}>
                    <Archive size={19} />
                    <div>
                      <strong>{dateTime(backup.createdAt)}</strong>
                      <small>
                        {backup.automatic ? "Automatic" : "Manual / safety"} ·{" "}
                        {(backup.bytes / 1024 / 1024).toFixed(2)} MB ·{" "}
                        {backup.name}
                      </small>
                    </div>
                    <Button
                      disabled={busy}
                      onClick={() => void restore(backup.name)}
                    >
                      Restore
                    </Button>
                  </div>
                ))
              ) : (
                <p className="muted">
                  No backups yet. Make a manual backup, or keep studying and
                  Tala will create one automatically.
                </p>
              )}
            </div>
          </section>
          <section className="panel settings-panel">
            <div className="panel-heading">
              <div>
                <h2>Collection health</h2>
                <p>
                  Check database relationships, scheduling states, and local
                  images.
                </p>
              </div>
              <CheckCircle2 size={21} />
            </div>
            <div className="inline">
              <Button busy={busy} onClick={integrity}>
                Run integrity check
              </Button>
              <Button onClick={diagnostics}>Export diagnostics</Button>
              <Button busy={busy} onClick={repair}>
                Rebuild indexes
              </Button>
            </div>
            {report && (
              <div className="integrity-result">
                <h3 className={report.healthy ? "text-teal" : "text-amber"}>
                  {report.healthy
                    ? "Your collection looks healthy"
                    : "Some items need attention"}
                </h3>
                {report.issues.map((issue, i) => (
                  <p key={i}>{issue}</p>
                ))}
                <p>
                  {number(report.missingMedia.length)} missing images ·{" "}
                  {number(report.unusedMedia.length)} unused images
                </p>
                {report.missingMedia.length > 0 && (
                  <details>
                    <summary>Missing image references</summary>
                    {report.missingMedia.map((id) => (
                      <code key={id}>{id}</code>
                    ))}
                    <p>
                      Reattach missing images from the editor or restore a
                      complete backup. Tala will not guess or delete damaged
                      records.
                    </p>
                  </details>
                )}
                {report.unusedMedia.length > 0 && (
                  <Button disabled={!report.healthy} onClick={cleanup}>
                    Remove unused images
                  </Button>
                )}
                {!report.healthy && (
                  <Button
                    busy={busy}
                    onClick={async () => {
                      const restored = await run(() =>
                        rpc({ action: "recover_collection" }),
                      );
                      if (restored) {
                        setReport(null);
                        setPreferences(
                          (await rpc({ action: "bootstrap" })).preferences,
                        );
                      }
                    }}
                  >
                    Recover from a backup
                  </Button>
                )}
              </div>
            )}
          </section>
        </>
      )}
      <footer className="page-footer">
        <span>Tala {version} · FSRS 6</span>
        <span>
          <i className="status-dot" />
          Offline, by design
        </span>
      </footer>
    </div>
  );
}
