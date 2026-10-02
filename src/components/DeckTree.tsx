import { useEffect, useMemo, useState } from "react";
import { ChevronRight, Play, Layers } from "lucide-react";
import type { Deck } from "../bindings/Deck";
import { useTala } from "../lib/context";
import { deckAncestors } from "../lib/decks";
import { number } from "../lib/format";
import { DeckMenu } from "../pages/Decks";

export function DeckTree({
  parentId = null,
  search = "",
  sort = "name",
}: {
  parentId?: string | null;
  search?: string;
  sort?: string;
}) {
  const { data, navigate, startStudy } = useTala();
  const [collapsed, setCollapsed] = useState<Set<string>>(() => {
    try {
      const saved = JSON.parse(
        localStorage.getItem("tala:collapsed-decks") || "null",
      );
      if (Array.isArray(saved)) return new Set(saved);
    } catch {
      /* A corrupt UI preference should not prevent opening the collection. */
    }
    return new Set(
      data.decks.filter((deck) => !deck.parentId).map((deck) => deck.id),
    );
  });
  useEffect(() => {
    localStorage.setItem(
      "tala:collapsed-decks",
      JSON.stringify([...collapsed]),
    );
  }, [collapsed]);
  const visible = useMemo(() => {
    if (!search.trim()) return new Set(data.decks.map((deck) => deck.id));
    const matches = data.decks.filter((deck) =>
      deck.path.toLowerCase().includes(search.trim().toLowerCase()),
    );
    return new Set(
      matches.flatMap((deck) => [
        deck.id,
        ...deckAncestors(data.decks, deck).map((parent) => parent.id),
      ]),
    );
  }, [data.decks, search]);
  const children = (parent: string | null) =>
    data.decks
      .filter((deck) => deck.parentId === parent && visible.has(deck.id))
      .sort((a, b) =>
        sort === "due"
          ? b.subtreeCounts.due - a.subtreeCounts.due ||
            a.name.localeCompare(b.name)
          : sort === "recent"
            ? b.updatedAt - a.updatedAt || a.name.localeCompare(b.name)
            : a.name.localeCompare(b.name),
      );
  function toggle(id: string) {
    setCollapsed((old) => {
      const next = new Set(old);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  }
  function row(deck: Deck, depth: number) {
    const descendants = children(deck.id);
    const open = !!search || !collapsed.has(deck.id);
    const counts = deck.subtreeCounts;
    return (
      <li
        role="treeitem"
        key={deck.id}
        tabIndex={0}
        aria-label={deck.name}
        aria-level={depth + 1}
        aria-expanded={descendants.length ? open : undefined}
        onKeyDown={(event) => {
          if (event.target !== event.currentTarget) return;
          if (event.key === "ArrowRight" && descendants.length && !open) {
            event.preventDefault();
            toggle(deck.id);
          } else if (event.key === "ArrowRight" && descendants.length && open) {
            event.preventDefault();
            event.currentTarget
              .querySelector<HTMLElement>('[role="group"] > [role="treeitem"]')
              ?.focus();
          } else if (event.key === "ArrowLeft" && descendants.length && open) {
            event.preventDefault();
            toggle(deck.id);
          } else if (event.key === "ArrowLeft") {
            event.preventDefault();
            event.currentTarget.parentElement
              ?.closest<HTMLElement>('[role="treeitem"]')
              ?.focus();
          } else if (
            ["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)
          ) {
            event.preventDefault();
            const rows = [
              ...event.currentTarget
                .closest('[role="tree"]')!
                .querySelectorAll<HTMLElement>('[role="treeitem"]'),
            ];
            const index = rows.indexOf(event.currentTarget);
            rows[
              event.key === "Home"
                ? 0
                : event.key === "End"
                  ? rows.length - 1
                  : Math.max(
                      0,
                      Math.min(
                        rows.length - 1,
                        index + (event.key === "ArrowDown" ? 1 : -1),
                      ),
                    )
            ]?.focus();
          } else if (event.key === "Enter") {
            event.preventDefault();
            void navigate({ page: "deck", id: deck.id });
          }
        }}
      >
        <div
          className="deck-tree-row"
          style={{ paddingLeft: `${12 + Math.min(depth, 6) * 24}px` }}
        >
          {descendants.length ? (
            <button
              type="button"
              className="icon-button tree-toggle"
              aria-label={`${open ? "Collapse" : "Expand"} ${deck.name}`}
              aria-expanded={open}
              onClick={() => toggle(deck.id)}
            >
              <ChevronRight size={17} className={open ? "expanded" : ""} />
            </button>
          ) : (
            <span className="tree-toggle" />
          )}
          <Layers size={19} className={`tree-deck-icon color-${deck.color}`} />
          <button
            type="button"
            className="deck-tree-name"
            aria-label={`Open ${deck.path}`}
            onClick={() => void navigate({ page: "deck", id: deck.id })}
            title={deck.path}
          >
            <strong>{deck.name}</strong>
            <small>
              {number(counts.total)} {counts.total === 1 ? "card" : "cards"}
              {descendants.length
                ? ` · ${number(descendants.length)} ${descendants.length === 1 ? "subdeck" : "subdecks"}`
                : ""}
            </small>
          </button>
          <span className="tree-due">
            {counts.due ? `${number(counts.due)} to study` : "Caught up"}
          </span>
          <button
            type="button"
            className="icon-button"
            aria-label={`Study ${deck.path}`}
            disabled={!counts.due}
            onClick={() => void startStudy(deck.id)}
          >
            <Play size={17} />
          </button>
          <DeckMenu deck={deck} />
        </div>
        {descendants.length && open ? (
          <ul role="group">
            {descendants.map((child) => row(child, depth + 1))}
          </ul>
        ) : null}
      </li>
    );
  }
  const roots = children(parentId);
  return roots.length ? (
    <ul className="deck-tree" role="tree" aria-label="Deck hierarchy">
      {roots.map((deck) => row(deck, 0))}
    </ul>
  ) : (
    <p className="muted">
      {search ? "No decks found. Try another name." : "No subdecks yet."}
    </p>
  );
}
