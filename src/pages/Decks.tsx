import { useState } from "react";
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
import {
  Button,
  Empty,
  Menu,
  Modal,
  PageHeader,
  Progress,
} from "../components/ui";
import { DeckArt } from "../components/DeckDialog";
import { DeckSettingsDialog } from "../components/SchedulingSettings";
import { DeckTree } from "../components/DeckTree";
import { branchIds, deckAncestors } from "../lib/decks";

export function DeckMenu({ deck }: { deck: Deck }) {
  const { navigate, editDeck, confirm, run, importCards, exportCards, data } =
    useTala();
  const [settings, setSettings] = useState(false);
  const [moving, setMoving] = useState(false);
  const [destination, setDestination] = useState("");
  const descendants = branchIds(data.decks, deck.id);
  const hasChildren = descendants.size > 1;
  async function remove(branch = false) {
    const count = branch ? deck.subtreeCounts.total : deck.total;
    const hasCards = count > 0;
    if (
      !(await confirm({
        title: `Delete ${branch ? "branch" : "deck"} “${deck.name}”?`,
        message: hasCards
          ? `${number(count)} cards will move to Recently Deleted. ${branch ? `${descendants.size} decks will be removed.` : hasChildren ? "Child decks will move up one level." : ""} Review history is kept until permanent deletion.`
          : branch
            ? "This branch will be removed."
            : hasChildren
              ? "This empty parent will be removed; child decks will move up one level."
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
          payload: { deckId: deck.id, moveTo: null, branch },
        }),
      "Deck deleted",
    );
    await navigate({ page: "decks" });
  }
  async function moveAndDelete() {
    const target = data.decks.find(
      (candidate) =>
        candidate.id === destination && !descendants.has(candidate.id),
    );
    if (!target) return;
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
            payload: { deckId: deck.id, moveTo: target.id, branch: false },
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
            label: "Add subdeck",
            icon: <Plus size={15} />,
            action: () => editDeck(undefined, deck.id),
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
            action: () => setMoving(true),
            disabled:
              !data.decks.some((candidate) => !descendants.has(candidate.id)) ||
              !deck.total,
            separator: true,
          },
          {
            label: "Delete deck",
            icon: <Trash2 size={15} />,
            action: () => void remove(false),
            danger: true,
          },
          ...(hasChildren
            ? [
                {
                  label: "Delete entire branch",
                  icon: <Trash2 size={15} />,
                  action: () => void remove(true),
                  danger: true,
                },
              ]
            : []),
        ]}
      />
      {moving && (
        <Modal
          title="Move cards and delete deck"
          description="Only cards directly in this deck will move. Its children will move up one level."
          open
          onClose={() => setMoving(false)}
        >
          <label className="field">
            <span>Destination deck</span>
            <select
              value={destination}
              onChange={(e) => setDestination(e.target.value)}
            >
              <option value="">Choose a deck</option>
              {data.decks
                .filter((candidate) => !descendants.has(candidate.id))
                .map((candidate) => (
                  <option key={candidate.id} value={candidate.id}>
                    {candidate.path}
                  </option>
                ))}
            </select>
          </label>
          <div className="modal-actions">
            <Button onClick={() => setMoving(false)}>Cancel</Button>
            <Button
              disabled={!destination}
              onClick={() => void moveAndDelete()}
            >
              Move and delete
            </Button>
          </div>
        </Modal>
      )}
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
            {deck.subtreeCounts.due
              ? `${number(deck.subtreeCounts.due)} to study`
              : "All caught up"}
          </span>
        </div>
        <div className="deck-tile-body">
          <h2>{deck.name}</h2>
          <p>
            {number(deck.subtreeCounts.total)}{" "}
            {deck.subtreeCounts.total === 1 ? "card" : "cards"}{" "}
            <ArrowUpRight size={17} />
          </p>
          <Progress
            value={
              deck.subtreeCounts.total
                ? deck.subtreeCounts.inReview / deck.subtreeCounts.total
                : 0
            }
            label={`${deck.name}: cards in Review`}
          />
          <small>{number(deck.subtreeCounts.inReview)} in Review</small>
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
        <DeckTree search={search} sort={sort} />
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
  const { data, navigate, startStudy, editDeck } = useTala();
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
  const counts = deck.subtreeCounts;
  const ancestors = deckAncestors(data.decks, deck);
  return (
    <div className="page">
      <button
        className="back-link"
        onClick={() => void navigate({ page: "decks" })}
      >
        ← All decks
      </button>
      <nav className="deck-breadcrumbs" aria-label="Deck path">
        {ancestors.map((parent) => (
          <button
            key={parent.id}
            onClick={() => void navigate({ page: "deck", id: parent.id })}
          >
            {parent.name} /
          </button>
        ))}
        <span>{deck.name}</span>
      </nav>
      <section className="deck-hero">
        <DeckArt deck={deck} mediaDir={data.mediaDir} />
        <div className="deck-hero-content">
          <span className="eyebrow">YOUR COLLECTION</span>
          <h1>{deck.name}</h1>
          <p>
            {number(counts.total)} cards · {number(counts.inReview)} in Review
          </p>
          <div className="inline">
            <Button
              variant="primary"
              disabled={!counts.due}
              onClick={() => void startStudy(deck.id)}
            >
              <Play size={17} />
              {counts.due
                ? `Study ${number(counts.due)} cards`
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
      <div className="section-heading">
        <h2>Subdecks</h2>
        <Button onClick={() => editDeck(undefined, deck.id)}>
          <Plus size={16} />
          Add subdeck
        </Button>
      </div>
      <DeckTree parentId={deck.id} />
      <div className="metrics-row">
        {[
          ["New available", counts.newCount, "violet"],
          ["Learning now", counts.learning, "amber"],
          ["Reviews due", counts.review, "teal"],
          ["Total cards", counts.total, "blue"],
        ].map(([label, value, color]) => (
          <div className="metric-card" key={label as string}>
            <span>{label}</span>
            <strong className={`text-${color}`}>
              {number(value as number)}
            </strong>
          </div>
        ))}
      </div>
      {counts.limitedNew + counts.limitedReview > 0 && (
        <div className="notice">
          Daily limits are holding back {number(counts.limitedNew)} new and{" "}
          {number(counts.limitedReview)} review cards. Learning repetitions
          remain available.{" "}
          <button onClick={() => setSettings(true)}>Adjust limits</button>
        </div>
      )}
      {!counts.total ? (
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
              {number(counts.inReview)} of {number(counts.total)} cards are in
              the Review stage.
            </p>
            <Progress
              value={counts.inReview / counts.total}
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
