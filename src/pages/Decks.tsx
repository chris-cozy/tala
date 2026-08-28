import { useMemo, useState } from "react";
import {
  Plus,
  Search,
  ArrowUpRight,
  Play,
  Pencil,
  Settings2,
  Library,
  Trash2,
  Download,
  Upload,
  Layers,
} from "lucide-react";
import type { Deck } from "../bindings/Deck";
import { useTala } from "../lib/context";
import { number } from "../lib/format";
import { rpc } from "../lib/api";
import { Button, Empty, Menu, PageHeader, Progress } from "../components/ui";
import { DeckArt } from "../components/DeckDialog";
import { DeckSettingsDialog } from "../components/SchedulingSettings";

export function DeckMenu({ deck }: { deck: Deck }) {
  const {
    navigate,
    editDeck,
    confirm,
    run,
    importCards,
    exportCards,
    ask,
    data,
  } = useTala();
  const [settings, setSettings] = useState(false);
  async function remove() {
    const hasCards = deck.total > 0;
    if (
      !(await confirm({
        title: `Delete “${deck.name}”?`,
        message: hasCards
          ? `${number(deck.total)} cards will move to Recently Deleted, along with their notes. You can restore them into another deck. Review history is kept until permanent deletion.`
          : "This empty deck will be removed.",
        confirm: "Delete deck",
        danger: true,
      }))
    )
      return;
    await run(
      () =>
        rpc({
          action: "delete_deck",
          payload: { deckId: deck.id, moveTo: null },
        }),
      "Deck deleted",
    );
    await navigate({ page: "decks" });
  }
  async function moveAndDelete() {
    const other = data.decks.filter((d) => d.id !== deck.id);
    const name = await ask(
      "Move cards and delete deck",
      `Destination deck name (${other.map((d) => d.name).join(", ")})`,
    );
    if (!name) return;
    const target = other.find(
      (d) => d.name.toLowerCase() === name.toLowerCase(),
    );
    if (!target) {
      await run(() =>
        Promise.reject(
          new Error("Choose the exact name of an existing destination deck."),
        ),
      );
      return;
    }
    if (
      await confirm({
        title: "Move cards and delete deck?",
        message: `Move all cards to ${target.name}, preserving their schedules, then delete ${deck.name}.`,
        confirm: "Move and delete",
        danger: true,
      })
    ) {
      await run(
        () =>
          rpc({
            action: "delete_deck",
            payload: { deckId: deck.id, moveTo: target.id },
          }),
        "Cards moved and deck deleted",
      );
      await navigate({ page: "decks" });
    }
  }
  return (
    <>
      <Menu
        label={`Options for ${deck.name}`}
        items={[
          {
            label: "Edit name & artwork",
            icon: <Pencil size={15} />,
            action: () => editDeck(deck),
          },
          {
            label: "Scheduling settings",
            icon: <Settings2 size={15} />,
            action: () => setSettings(true),
          },
          {
            label: "Browse cards",
            icon: <Library size={15} />,
            action: () => void navigate({ page: "browse", deckId: deck.id }),
          },
          {
            label: "Import cards",
            icon: <Upload size={15} />,
            action: () => importCards(deck.id),
            separator: true,
          },
          {
            label: "Export deck",
            icon: <Download size={15} />,
            action: () => exportCards(deck.id),
          },
          {
            label: "Move cards & delete",
            action: () => void moveAndDelete(),
            disabled: data.decks.length < 2 || !deck.total,
            separator: true,
          },
          {
            label: "Delete deck",
            icon: <Trash2 size={15} />,
            action: () => void remove(),
            danger: true,
          },
        ]}
      />
      {settings && (
        <DeckSettingsDialog deck={deck} onClose={() => setSettings(false)} />
      )}
    </>
  );
}
export function DeckTile({ deck }: { deck: Deck }) {
  const { data, navigate } = useTala();
  return (
    <article className="deck-tile">
      <button
        className="deck-open"
        onClick={() => void navigate({ page: "deck", id: deck.id })}
        aria-label={`Open ${deck.name}`}
      >
        <DeckArt deck={deck} mediaDir={data.mediaDir} />
        <div className="deck-tile-top">
          <span className="deck-due">
            {deck.due ? `${number(deck.due)} to study` : "All caught up"}
          </span>
        </div>
        <div className="deck-tile-body">
          <h2>{deck.name}</h2>
          <p>
            {number(deck.total)} {deck.total === 1 ? "card" : "cards"}{" "}
            <ArrowUpRight size={17} />
          </p>
          <Progress
            value={deck.total ? deck.inReview / deck.total : 0}
            label={`${deck.name}: cards in Review`}
          />
          <small>{number(deck.inReview)} in Review</small>
        </div>
      </button>
      <div className="deck-tile-menu">
        <DeckMenu deck={deck} />
      </div>
    </article>
  );
}
export default function Decks() {
  const { data, editDeck, importCards } = useTala();
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState("name");
  const decks = useMemo(
    () =>
      data.decks
        .filter((d) => d.name.toLowerCase().includes(search.toLowerCase()))
        .sort((a, b) =>
          sort === "due"
            ? b.due - a.due
            : sort === "recent"
              ? b.updatedAt - a.updatedAt
              : a.name.localeCompare(b.name),
        ),
    [data.decks, search, sort],
  );
  return (
    <div className="page">
      <PageHeader
        eyebrow="YOUR KNOWLEDGE, COLLECTED"
        title="Decks"
        subtitle="Small collections. Lasting understanding."
        actions={
          <>
            <Button onClick={() => importCards()}>
              <Upload size={16} />
              Import
            </Button>
            <Button variant="primary" onClick={() => editDeck()}>
              <Plus size={17} />
              New deck
            </Button>
          </>
        }
      />
      <div className="library-toolbar">
        <div className="search-field">
          <Search size={17} />
          <input
            aria-label="Search decks"
            placeholder="Find a deck…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <kbd>⌘ F</kbd>
        </div>
        <div className="inline">
          <span className="muted small">{number(data.decks.length)} decks</span>
          <select
            aria-label="Sort decks"
            value={sort}
            onChange={(e) => setSort(e.target.value)}
          >
            <option value="name">Name</option>
            <option value="due">Cards due</option>
            <option value="recent">Recently updated</option>
          </select>
        </div>
      </div>
      {!data.decks.length ? (
        <Empty
          title="Make room for what matters"
          icon={<Layers size={30} />}
          action={
            <Button variant="primary" onClick={() => editDeck()}>
              <Plus size={17} />
              Create your first deck
            </Button>
          }
        >
          A language, a subject, or something you’re curious about. Start with a
          deck, then add a few cards.
        </Empty>
      ) : (
        <div className="deck-grid">
          {decks.map((deck) => (
            <DeckTile key={deck.id} deck={deck} />
          ))}
          {!search && (
            <button className="new-deck-tile" onClick={() => editDeck()}>
              <span>
                <Plus size={24} />
              </span>
              New deck<small>A little more room to grow</small>
            </button>
          )}
          {!decks.length && (
            <Empty title="No decks found">Try another name.</Empty>
          )}
        </div>
      )}
      <footer className="page-footer">
        <span>
          {number(data.decks.reduce((n, d) => n + d.total, 0))} cards in your
          collection
        </span>
        <span>
          <i className="status-dot" />
          Saved on this device
        </span>
      </footer>
    </div>
  );
}
export function DeckOverview({ id }: { id: string }) {
  const { data, navigate, startStudy } = useTala();
  const [settings, setSettings] = useState(false);
  const deck = data.decks.find((d) => d.id === id);
  if (!deck)
    return (
      <Empty
        title="Deck not found"
        action={
          <Button onClick={() => void navigate({ page: "decks" })}>
            Back to decks
          </Button>
        }
      />
    );
  return (
    <div className="page">
      <button
        className="back-link"
        onClick={() => void navigate({ page: "decks" })}
      >
        ← All decks
      </button>
      <section className="deck-hero">
        <DeckArt deck={deck} mediaDir={data.mediaDir} />
        <div className="deck-hero-content">
          <span className="eyebrow">YOUR COLLECTION</span>
          <h1>{deck.name}</h1>
          <p>
            {number(deck.total)} cards · {number(deck.inReview)} in Review
          </p>
          <div className="inline">
            <Button
              variant="primary"
              disabled={!deck.due}
              onClick={() => void startStudy(deck.id)}
            >
              <Play size={17} />
              {deck.due
                ? `Study ${number(deck.due)} cards`
                : "Caught up for now"}
            </Button>
            <Button
              onClick={() => void navigate({ page: "editor", deckId: deck.id })}
            >
              <Plus size={17} />
              Add card
            </Button>
          </div>
        </div>
        <div className="hero-menu">
          <DeckMenu deck={deck} />
        </div>
      </section>
      <div className="metrics-row">
        {[
          ["New available", deck.newCount, "violet"],
          ["Learning now", deck.learning, "amber"],
          ["Reviews due", deck.review, "teal"],
          ["Total cards", deck.total, "blue"],
        ].map(([label, value, color]) => (
          <div className="metric-card" key={label as string}>
            <span>{label}</span>
            <strong className={`text-${color}`}>
              {number(value as number)}
            </strong>
          </div>
        ))}
      </div>
      {deck.limitedNew + deck.limitedReview > 0 && (
        <div className="notice">
          Daily limits are holding back {number(deck.limitedNew)} new and{" "}
          {number(deck.limitedReview)} review cards. Learning repetitions remain
          available.{" "}
          <button onClick={() => setSettings(true)}>Adjust limits</button>
        </div>
      )}
      {!deck.total ? (
        <Empty
          title="Your first card is a small beginning"
          action={
            <Button
              variant="primary"
              onClick={() => void navigate({ page: "editor", deckId: deck.id })}
            >
              <Plus size={16} />
              Add a card
            </Button>
          }
        >
          Write a question you want to remember, and its answer.
        </Empty>
      ) : (
        <div className="panel deck-overview-panel">
          <div>
            <h2>A little progress, every day</h2>
            <p>
              {number(deck.inReview)} of {number(deck.total)} cards are in the
              Review stage.
            </p>
            <Progress
              value={deck.inReview / deck.total}
              label="Cards in Review"
            />
          </div>
          <div className="inline">
            <Button
              onClick={() => void navigate({ page: "browse", deckId: deck.id })}
            >
              <Library size={16} />
              Browse cards
            </Button>
            <Button onClick={() => setSettings(true)}>
              <Settings2 size={16} />
              Scheduling
            </Button>
          </div>
        </div>
      )}
      {settings && (
        <DeckSettingsDialog deck={deck} onClose={() => setSettings(false)} />
      )}
    </div>
  );
}
