import { useState } from "react";
import type { Deck } from "../bindings/Deck";
import type { DeckSettings } from "../bindings/DeckSettings";
import { Field, useTala } from "../lib/context";
import { rpc } from "../lib/api";
import { Button, Modal } from "./ui";
export function formatSteps(steps: number[]) {
  return steps
    .map((s) =>
      s % 86400 === 0
        ? `${s / 86400}d`
        : s % 3600 === 0
          ? `${s / 3600}h`
          : s % 60 === 0
            ? `${s / 60}m`
            : `${s}s`,
    )
    .join(" ");
}
/** Convert the editable duration list to seconds; Rust enforces scheduling limits on save. */
export function parseSteps(text: string) {
  if (!text.trim()) return [];
  const steps = text
    .trim()
    .split(/[\s,]+/)
    .map((part) => {
      const match = part.match(/^(\d+(?:\.\d+)?)([smhd]?)$/i);
      if (!match) throw new Error("Use durations such as 1m 10m, or 1h 1d.");
      return Math.round(
        Number(match[1]) *
          ({ s: 1, m: 60, h: 3600, d: 86400 }[match[2].toLowerCase() || "m"] ??
            60),
      );
    });
  if (
    steps.length > 20 ||
    steps.some((s, i) => s < 1 || s > 604800 || (i > 0 && s <= steps[i - 1]))
  )
    throw new Error("Use increasing steps between 1 second and 7 days.");
  return steps;
}
export function SchedulingForm({
  value,
  onChange,
  onValidity,
}: {
  value: DeckSettings;
  onChange: (value: DeckSettings) => void;
  onValidity?: (valid: boolean) => void;
}) {
  const [learning, setLearning] = useState(formatSteps(value.learningSteps));
  const [relearning, setRelearning] = useState(
    formatSteps(value.relearningSteps),
  );
  const [stepErrors, setStepErrors] = useState<Record<string, string>>({});
  function update<K extends keyof DeckSettings>(key: K, next: DeckSettings[K]) {
    onChange({ ...value, [key]: next });
  }
  function steps(key: "learningSteps" | "relearningSteps", raw: string) {
    if (key === "learningSteps") setLearning(raw);
    else setRelearning(raw);
    const errors = { ...stepErrors };
    try {
      update(key, parseSteps(raw));
      delete errors[key];
    } catch (e) {
      errors[key] = (e as Error).message;
    }
    setStepErrors(errors);
    onValidity?.(!Object.keys(errors).length);
  }
  return (
    <div className="scheduling-form">
      <h3>Daily rhythm</h3>
      <div className="form-grid">
        <Field
          label="New cards per day"
          hint="Learning repetitions do not count toward this limit."
        >
          <input
            type="number"
            min={0}
            max={100000}
            value={value.newLimit}
            onChange={(e) => update("newLimit", Number(e.target.value))}
          />
        </Field>
        <Field label="Review cards per day">
          <input
            type="number"
            min={0}
            max={100000}
            value={value.reviewLimit}
            onChange={(e) => update("reviewLimit", Number(e.target.value))}
          />
        </Field>
      </div>
      <h3>Memory & repetition</h3>
      <div className="form-grid">
        <Field
          label="Desired retention (%)"
          hint="A higher target means more frequent reviews."
        >
          <input
            type="number"
            min={70}
            max={99}
            step={1}
            value={Math.round(value.retention * 100)}
            onChange={(e) => update("retention", Number(e.target.value) / 100)}
          />
        </Field>
        <Field label="Maximum interval (days)">
          <input
            type="number"
            min={1}
            max={36500}
            value={value.maximumInterval}
            onChange={(e) => update("maximumInterval", Number(e.target.value))}
          />
        </Field>
        <Field
          label="Learning steps"
          hint={
            stepErrors.learningSteps ||
            "s = seconds, m = minutes, h = hours, d = days. Empty skips steps."
          }
        >
          <input
            aria-invalid={!!stepErrors.learningSteps}
            value={learning}
            onChange={(e) => steps("learningSteps", e.target.value)}
          />
        </Field>
        <Field
          label="Relearning steps"
          hint={
            stepErrors.relearningSteps ||
            "Repetitions after forgetting a learned card."
          }
        >
          <input
            aria-invalid={!!stepErrors.relearningSteps}
            value={relearning}
            onChange={(e) => steps("relearningSteps", e.target.value)}
          />
        </Field>
      </div>
      <h3>Study ordering</h3>
      <div className="form-grid three">
        <Field label="New card order">
          <select
            value={value.newOrder}
            onChange={(e) => update("newOrder", e.target.value)}
          >
            <option value="created">Creation order</option>
            <option value="random">Random</option>
          </select>
        </Field>
        <Field label="New cards appear">
          <select
            value={value.newPlacement}
            onChange={(e) => update("newPlacement", e.target.value)}
          >
            <option value="after">After reviews</option>
            <option value="before">Before reviews</option>
            <option value="mix">Mixed with reviews</option>
          </select>
        </Field>
        <Field label="Review order">
          <select
            value={value.reviewOrder}
            onChange={(e) => update("reviewOrder", e.target.value)}
          >
            <option value="due">Due date</option>
            <option value="random">Random</option>
          </select>
        </Field>
      </div>
      <h3>Difficult cards</h3>
      <div className="form-grid">
        <Field label="Leech threshold (lapses)">
          <input
            type="number"
            min={1}
            value={value.leechThreshold}
            onChange={(e) => update("leechThreshold", Number(e.target.value))}
          />
        </Field>
        <label className="check-label">
          <input
            type="checkbox"
            checked={value.suspendLeeches}
            onChange={(e) => update("suspendLeeches", e.target.checked)}
          />
          <span>
            Automatically suspend leeches
            <small>Otherwise, mark them for your attention.</small>
          </span>
        </label>
      </div>
    </div>
  );
}
export function DeckSettingsDialog({
  deck,
  onClose,
}: {
  deck: Deck;
  onClose: () => void;
}) {
  const { run } = useTala();
  const [settings, setSettings] = useState(deck.settings);
  const [valid, setValid] = useState(true);
  const [busy, setBusy] = useState(false);
  async function save() {
    setBusy(true);
    const result = await run(
      () =>
        rpc({
          action: "save_deck",
          payload: {
            id: deck.id,
            name: deck.name,
            cover: deck.cover,
            color: deck.color,
            settings,
          },
        }),
      "Scheduling settings saved",
    );
    setBusy(false);
    if (result) onClose();
  }
  return (
    <Modal
      title={`${deck.name} · Scheduling`}
      description="Changes apply to future reviews. Existing due dates remain unchanged unless you explicitly reschedule cards in Browse."
      wide
      open
      onClose={onClose}
    >
      <SchedulingForm
        value={settings}
        onChange={setSettings}
        onValidity={setValid}
      />
      <div className="modal-actions">
        <Button onClick={onClose}>Cancel</Button>
        <Button variant="primary" disabled={!valid} busy={busy} onClick={save}>
          Save settings
        </Button>
      </div>
    </Modal>
  );
}
