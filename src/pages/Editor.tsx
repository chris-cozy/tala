import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeft,
  ArrowDown,
  ArrowUp,
  Keyboard,
  Eye,
  Save,
  Plus,
  X,
  ArrowDownUp,
} from "lucide-react";
import type { Behavior } from "../bindings/Behavior";
import type { CardView } from "../bindings/CardView";
import { rpc } from "../lib/api";
import { useTala, Field } from "../lib/context";
import { Button, Empty, ErrorPanel, Loading, Modal } from "../components/ui";
import {
  AnswerComparison,
  ContentRender,
  RichField,
  emptyDoc,
  extractText,
  hasContent,
  type Doc,
} from "../components/RichContent";

export function TagsInput({
  value,
  onChange,
  suggestions,
}: {
  value: string[];
  onChange: (value: string[]) => void;
  suggestions: string[];
}) {
  const [text, setText] = useState("");
  function add() {
    const tag = text.trim().toLowerCase();
    if (tag && !value.includes(tag)) onChange([...value, tag]);
    setText("");
  }
  return (
    <div className="tags-input">
      {value.map((tag) => (
        <span className="tag" key={tag}>
          {tag}
          <button
            type="button"
            aria-label={`Remove tag ${tag}`}
            onClick={() => onChange(value.filter((t) => t !== tag))}
          >
            <X size={12} />
          </button>
        </span>
      ))}
      <input
        aria-label="Add tags"
        list="tag-suggestions"
        placeholder={value.length ? "Add another…" : "Add tags…"}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ";") {
            e.preventDefault();
            add();
          }
        }}
        onBlur={add}
      />
      <datalist id="tag-suggestions">
        {suggestions
          .filter((t) => !value.includes(t))
          .map((t) => (
            <option key={t} value={t} />
          ))}
      </datalist>
      {text.trim() && (
        <button
          type="button"
          className="icon-button"
          onClick={add}
          aria-label="Add tag"
        >
          <Plus size={15} />
        </button>
      )}
    </div>
  );
}
export function CardPreview({
  front,
  back,
  behavior,
  onClose,
}: {
  front: Doc;
  back: Doc;
  behavior: Behavior;
  onClose: () => void;
}) {
  const { data } = useTala();
  const [revealed, setRevealed] = useState(false);
  const [answer, setAnswer] = useState("");
  return (
    <Modal
      title="Card preview"
      description="This is how your card will appear during study. Previewing never changes its schedule."
      wide
      open
      onClose={onClose}
    >
      <div className="preview-card">
        <span className="eyebrow">{revealed ? "ANSWER" : "QUESTION"}</span>
        <ContentRender
          directory={data.mediaDir}
          value={
            revealed
              ? behavior === "reversed"
                ? front
                : back
              : behavior === "reversed"
                ? back
                : front
          }
        />
        {behavior === "typed" && !revealed && (
          <input
            aria-label="Preview answer"
            placeholder="Type your answer…"
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") setRevealed(true);
            }}
          />
        )}{" "}
        {revealed && behavior === "typed" && (
          <AnswerComparison answer={answer} expected={extractText(back)} />
        )}
      </div>
      <div className="modal-actions">
        <Button onClick={() => setRevealed(!revealed)}>
          {revealed ? "Show question" : "Reveal answer"}
        </Button>
        <Button variant="primary" onClick={onClose}>
          Back to editor
        </Button>
      </div>
    </Modal>
  );
}
export default function EditorPage({
  cardId,
  deckId,
}: {
  cardId?: string;
  deckId?: string;
}) {
  const { data, editDeck } = useTala();
  const query = useQuery({
    queryKey: ["card", cardId],
    queryFn: () => rpc({ action: "get_card", payload: { cardId: cardId! } }),
    enabled: !!cardId,
  });
  if (!data.decks.length)
    return (
      <Empty
        title="Every card needs a home"
        action={
          <Button variant="primary" onClick={() => editDeck()}>
            <Plus size={16} />
            Create a deck
          </Button>
        }
      >
        Create your first deck, then start adding cards.
      </Empty>
    );
  if (cardId && query.isPending) return <Loading />;
  if (query.error)
    return (
      <ErrorPanel
        message={String(query.error)}
        retry={() => void query.refetch()}
      />
    );
  return (
    <EditorForm
      key={cardId ?? `new-${deckId ?? ""}`}
      card={query.data}
      initialDeck={deckId}
    />
  );
}
function EditorForm({
  card,
  initialDeck,
}: {
  card?: CardView;
  initialDeck?: string;
}) {
  const { data, run, navigate, setDirty, notify } = useTala();
  const [front, setFront] = useState<Doc>((card?.front as Doc) ?? emptyDoc());
  const [back, setBack] = useState<Doc>((card?.back as Doc) ?? emptyDoc());
  const [behavior, setBehavior] = useState<Behavior>(
    card?.behavior ?? "normal",
  );
  const [deck, setDeck] = useState(
    card?.deckId ?? initialDeck ?? data.decks[0].id,
  );
  const [tags, setTags] = useState(card?.tags ?? []);
  const [preview, setPreview] = useState(false);
  const [busy, setBusy] = useState(false);
  const [addAnother, setAddAnother] = useState(!card);
  const [count, setCount] = useState(0);
  const [dirty, dirtyState] = useState(false);
  function change<T>(set: (value: T) => void, value: T) {
    set(value);
    dirtyState(true);
    setDirty(true);
  }
  useEffect(() => () => setDirty(false), [setDirty]);
  async function save() {
    if (!hasContent(front) || !hasContent(back)) {
      notify("Add content to both Front and Back before saving.", true);
      return;
    }
    setBusy(true);
    const result = await run(
      () =>
        rpc({
          action: "save_note",
          payload: {
            id: card?.noteId ?? null,
            deckId: deck,
            front: front as CardView["front"],
            back: back as CardView["back"],
            behavior,
            tags,
          },
        }),
      card ? "Card updated · schedule preserved" : "Card added",
    );
    setBusy(false);
    if (result) {
      dirtyState(false);
      setDirty(false);
      if (!card && addAnother) {
        setFront(emptyDoc());
        setBack(emptyDoc());
        setCount((n) => n + 1);
      } else await navigate({ page: "deck", id: deck });
    }
  }
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (!busy) void save();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
  return (
    <div className="page editor-page">
      <header className="editor-header">
        <div className="inline">
          <Button
            variant="ghost"
            aria-label="Back from editor"
            onClick={() => void navigate({ page: "deck", id: deck })}
          >
            <ArrowLeft size={18} />
          </Button>
          <div>
            <h1>{card ? "Edit card" : "Add a card"}</h1>
            <p>
              {card
                ? "Refine the thought. Keep the progress."
                : count
                  ? `${count} added this session. Keep going.`
                  : "Make something worth remembering."}
            </p>
          </div>
        </div>
        <div className="inline">
          <span className="small muted">
            {dirty ? "Unsaved changes" : card ? "Saved" : ""}
          </span>
          <Button onClick={() => setPreview(true)}>
            <Eye size={16} />
            Preview
          </Button>
          <Button variant="primary" busy={busy} onClick={save}>
            <Save size={16} />
            {card ? "Save changes" : "Save card"}
            <kbd>⌘ S</kbd>
          </Button>
        </div>
      </header>
      <div className="editor-config">
        <Field label="Card behavior">
          <div className="behavior-switch">
            {(
              [
                { value: "normal", label: "Normal", icon: ArrowDown },
                { value: "reversed", label: "Reversed", icon: ArrowUp },
                { value: "typed", label: "Type in the Answer", icon: Keyboard },
              ] as const
            ).map((item) => (
              <button
                key={item.value}
                aria-pressed={behavior === item.value}
                className={behavior === item.value ? "selected" : ""}
                onClick={() => change(setBehavior, item.value)}
              >
                <item.icon size={16} />
                {item.label}
              </button>
            ))}
          </div>
        </Field>
        <Field label="Deck">
          <select
            value={deck}
            onChange={(e) => change(setDeck, e.target.value)}
          >
            {data.decks.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name}
              </option>
            ))}
          </select>
        </Field>
      </div>
      <section className="editor-sides">
        <div className="editor-field-label">
          <span>FRONT</span>
          <small>
            {behavior === "reversed"
              ? "Shown as the answer"
              : "Start with a question"}
          </small>
        </div>
        <RichField
          value={front}
          onChange={(doc) => change(setFront, doc)}
          label="Front"
        />
        <div className="side-divider">
          <button
            className="icon-button"
            aria-label="Swap Front and Back"
            onClick={() => {
              setFront(back);
              setBack(front);
              dirtyState(true);
              setDirty(true);
            }}
          >
            <ArrowDownUp size={15} />
          </button>
        </div>
        <div className="editor-field-label">
          <span>BACK</span>
          <small>
            {behavior === "reversed"
              ? "Shown as the question"
              : "Add the answer"}
          </small>
        </div>
        <RichField
          value={back}
          onChange={(doc) => change(setBack, doc)}
          label="Back"
        />
      </section>
      <footer className="editor-footer">
        <Field label="Tags">
          <TagsInput
            value={tags}
            onChange={(value) => change(setTags, value)}
            suggestions={data.tags}
          />
        </Field>
        {!card && (
          <label className="check-label">
            <input
              type="checkbox"
              checked={addAnother}
              onChange={(e) => setAddAnother(e.target.checked)}
            />
            Add another after saving
          </label>
        )}
      </footer>
      <p className="editor-tip">
        Paste or drop images directly into either side. Equations and images
        stay available offline.
      </p>
      {preview && (
        <CardPreview
          front={front}
          back={back}
          behavior={behavior}
          onClose={() => setPreview(false)}
        />
      )}
    </div>
  );
}
