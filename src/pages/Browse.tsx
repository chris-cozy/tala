import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  Search,
  Tag,
  FolderInput,
  Trash2,
  Download,
  Plus,
  ArrowUpDown,
  X,
  Pencil,
  SlidersHorizontal,
} from "lucide-react";
import type { CardView } from "../bindings/CardView";
import type { BrowseQuery } from "../bindings/BrowseQuery";
import { rpc, errorMessage } from "../lib/api";
import { useTala } from "../lib/context";
import {
  behaviorLabel,
  cardState,
  dateTime,
  number,
  isTyping,
} from "../lib/format";
import {
  Button,
  Empty,
  ErrorPanel,
  Loading,
  Menu,
  Modal,
  PageHeader,
} from "../components/ui";
import { CardDetails } from "../components/CardDetails";
const column = createColumnHelper<CardView>();
export default function BrowsePage({
  deckId,
  trash,
  tag,
}: {
  deckId?: string;
  trash?: boolean;
  tag?: string;
}) {
  const { data, run, ask, confirm, navigate, exportCards } = useTala();
  const [search, setSearch] = useState("");
  const [debounced, setDebounced] = useState("");
  const [onlyThisDeck, setOnlyThisDeck] = useState(false);
  const [deck, setDeck] = useState(deckId ?? "");
  const [selectedTag, setTag] = useState(tag ?? "");
  const [state, setState] = useState("");
  const [leech, setLeech] = useState(false);
  const [sort, setSort] = useState("created");
  const [descending, setDescending] = useState(false);
  const [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [detail, setDetail] = useState<string | null>(null);
  const [tagManager, setTagManager] = useState(false);
  const [move, setMove] = useState(false);
  const [destination, setDestination] = useState(data.decks[0]?.id ?? "");
  const scroll = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(search), 180);
    return () => clearTimeout(timer);
  }, [search]);
  useEffect(() => {
    // A new filter must not keep hidden selections that a later bulk action could change.
    setOffset(0);
    setSelected(new Set());
  }, [debounced, deck, onlyThisDeck, selectedTag, state, leech, trash]);
  const filter: BrowseQuery = {
    search: debounced,
    deck: deck || null,
    onlyThisDeck,
    tag: selectedTag || null,
    state: state || null,
    leech,
    trash: !!trash,
    sort,
    descending,
    offset,
    limit: 100,
  };
  const query = useQuery({
    queryKey: ["browse", filter],
    queryFn: () => rpc({ action: "browse", payload: filter }),
  });
  const cards = query.data?.cards ?? [];
  function toggle(id: string) {
    setSelected((old) => {
      const next = new Set(old);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  }
  function sortBy(name: string) {
    if (name === sort) setDescending(!descending);
    else {
      setSort(name);
      setDescending(false);
    }
  }
  async function bulk(action: string, value: string | null = null) {
    const result = await run(
      () =>
        rpc({
          action: "bulk",
          payload: { ids: Array.from(selected), action, value },
        }),
      `${number(selected.size)} cards updated`,
    );
    if (result !== undefined) setSelected(new Set());
  }
  async function editTags(action: "add_tag" | "remove_tag") {
    const value = await ask(
      action === "add_tag" ? "Add a tag" : "Remove a tag",
      "Tag name",
    );
    if (value) await bulk(action, value);
  }
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        isTyping(event.target) ||
        document.querySelector('[role="dialog"]') ||
        !selected.size ||
        trash
      )
        return;
      const key = event.key.toLowerCase();
      if ((key === "e" || key === "enter") && selected.size === 1) {
        event.preventDefault();
        void navigate({ page: "editor", cardId: [...selected][0] });
      }
      if (key === "s" || key === "b") {
        event.preventDefault();
        void bulk(key === "s" ? "suspend" : "bury");
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  });
  async function destructive(
    action: "delete" | "purge" | "reset" | "reschedule",
  ) {
    const options = {
      delete: [
        "Move notes to Recently Deleted?",
        "Selected notes and their cards leave normal study. Schedules and history are retained; you can restore the notes later.",
        "Move to Recently Deleted",
      ],
      purge: [
        "Permanently delete these notes?",
        "Their cards, scheduling data, and review history will be permanently removed from this collection. Existing backups are unchanged. This cannot be undone.",
        "Permanently delete",
      ],
      reset: [
        "Reset selected cards to New?",
        "Their current FSRS memory state will be cleared. Lifetime history and counts remain. You can undo this operation.",
        "Reset to New",
      ],
      reschedule: [
        "Recalculate due dates?",
        "Review cards will be rescheduled using their existing memory state and current deck settings. Other states are unchanged. No review history is added.",
        "Reschedule",
      ],
    }[action];
    if (
      await confirm({
        title: options[0],
        message: options[1],
        confirm: options[2],
        danger: action !== "reschedule",
      })
    )
      await bulk(action);
  }
  async function setDue() {
    const date = await ask(
      "Set a due date",
      "Date (YYYY-MM-DD)",
      new Date().toLocaleDateString("en-CA"),
    );
    if (!date) return;
    const parsed = new Date(`${date}T00:00:00`);
    if (
      !/^\d{4}-\d{2}-\d{2}$/.test(date) ||
      !Number.isFinite(parsed.getTime())
    ) {
      await run(() =>
        Promise.reject(new Error("Enter a valid date in YYYY-MM-DD format.")),
      );
      return;
    }
    await bulk("set_due", String(Math.floor(parsed.getTime() / 1000)));
  }
  const columns = useMemo(
    () => [
      column.display({
        id: "select",
        size: 42,
        header: () => (
          <input
            type="checkbox"
            aria-label="Select visible cards"
            checked={!!cards.length && cards.every((c) => selected.has(c.id))}
            onChange={(e) =>
              setSelected((old) => {
                const next = new Set(old);
                cards.forEach((c) =>
                  e.target.checked ? next.add(c.id) : next.delete(c.id),
                );
                return next;
              })
            }
          />
        ),
        cell: (info) => (
          <input
            type="checkbox"
            aria-label={`Select ${info.row.original.frontText}`}
            checked={selected.has(info.row.original.id)}
            onChange={() => toggle(info.row.original.id)}
          />
        ),
      }),
      column.accessor("frontText", {
        id: "front",
        size: 260,
        header: "Front",
        cell: (info) => (
          <button
            className="browser-front"
            onClick={() => setDetail(info.row.original.id)}
          >
            {info.getValue() || "Image / audio / equation"}
          </button>
        ),
      }),
      column.accessor("backText", {
        id: "back",
        size: 190,
        header: "Back",
        cell: (info) => (
          <span className="muted">
            {info.getValue() || "Image / audio / equation"}
          </span>
        ),
      }),
      column.accessor("deckName", {
        id: "deck",
        size: 155,
        header: "Deck",
        cell: (info) =>
          data.decks.find((deck) => deck.id === info.row.original.deckId)
            ?.path ?? info.getValue(),
      }),
      column.accessor("behavior", {
        id: "behavior",
        size: 140,
        header: "Behavior",
        cell: (info) => behaviorLabel(info.getValue()),
      }),
      column.accessor("tags", {
        id: "tags",
        size: 155,
        header: "Tags",
        cell: (info) => (
          <span className="table-tags">
            {info.getValue().map((t) => (
              <span className="tag" key={t}>
                {t}
              </span>
            ))}
          </span>
        ),
      }),
      column.display({
        id: "state",
        size: 115,
        header: "State",
        cell: (info) => (
          <span className={`state-badge state-${cardState(info.row.original)}`}>
            {cardState(info.row.original)}
          </span>
        ),
      }),
      column.display({
        id: "due",
        size: 110,
        header: "Due",
        cell: (info) => dateTime(info.row.original.schedule.due, true),
      }),
      column.display({
        id: "reviews",
        size: 80,
        header: "Reviews",
        cell: (info) => number(info.row.original.schedule.reviewCount),
      }),
      column.display({
        id: "lapses",
        size: 75,
        header: "Lapses",
        cell: (info) => number(info.row.original.schedule.lapses),
      }),
      column.accessor("leech", {
        id: "leech",
        size: 75,
        header: "Leech",
        cell: (info) =>
          info.getValue() ? <span className="text-amber">Yes</span> : "—",
      }),
    ],
    [cards, selected, data.decks],
  );
  const table = useReactTable({
    data: cards,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });
  const rows = table.getRowModel().rows;
  const virtual = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroll.current,
    estimateSize: () => 54,
    overscan: 8,
  });
  const widths = table
    .getAllLeafColumns()
    .map((c) => `${c.getSize()}px`)
    .join(" ");
  return (
    <div className="page browse-page">
      <PageHeader
        eyebrow="MAKE YOUR KNOWLEDGE YOUR OWN"
        title={trash ? "Recently Deleted" : "Card browser"}
        subtitle={
          trash
            ? "Restore notes with their original schedules, or remove them permanently."
            : "Find, refine, and organize the things you’re learning."
        }
        actions={
          <>
            {trash ? (
              <Button onClick={() => void navigate({ page: "browse" })}>
                Back to cards
              </Button>
            ) : (
              <>
                <Button onClick={() => setTagManager(true)}>
                  <Tag size={16} />
                  Manage tags
                </Button>
                <Button
                  variant="primary"
                  onClick={() =>
                    void navigate({ page: "editor", deckId: deck || undefined })
                  }
                >
                  <Plus size={16} />
                  Add card
                </Button>
              </>
            )}
          </>
        }
      />
      <div className="browser-filters">
        <div className="search-field">
          <Search size={16} />
          <input
            aria-label="Search cards"
            placeholder="Search questions and answers…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <select
          aria-label="Filter by deck"
          value={deck}
          onChange={(e) => setDeck(e.target.value)}
        >
          <option value="">All decks</option>
          {data.decks.map((d) => (
            <option key={d.id} value={d.id}>
              {d.path}
            </option>
          ))}
        </select>
        <select
          aria-label="Filter by tag"
          value={selectedTag}
          onChange={(e) => setTag(e.target.value)}
        >
          <option value="">All tags</option>
          {data.tags.map((t) => (
            <option key={t}>{t}</option>
          ))}
        </select>
        <select
          aria-label="Filter by state"
          value={state}
          onChange={(e) => setState(e.target.value)}
        >
          <option value="">All states</option>
          {[
            "new",
            "learning",
            "review",
            "relearning",
            "suspended",
            "buried",
          ].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
        {deck && (
          <label className="check-label compact-check">
            <input
              type="checkbox"
              checked={onlyThisDeck}
              onChange={(e) => setOnlyThisDeck(e.target.checked)}
            />
            Only this deck
          </label>
        )}
        <label className="check-label compact-check">
          <input
            type="checkbox"
            checked={leech}
            onChange={(e) => setLeech(e.target.checked)}
          />
          Leeches
        </label>
      </div>
      <div className="browser-actions">
        <div className="inline">
          <span className="small muted">
            {selected.size
              ? `${number(selected.size)} selected`
              : `${number(query.data?.total ?? 0)} cards`}
          </span>
          {selected.size > 0 && (
            <button
              className="icon-button"
              aria-label="Clear selection"
              onClick={() => setSelected(new Set())}
            >
              <X size={15} />
            </button>
          )}
        </div>
        {selected.size > 0 ? (
          <div className="inline">
            {trash ? (
              <>
                <Button onClick={() => setMove(true)}>Restore to deck</Button>
                <Button
                  variant="danger"
                  onClick={() => void destructive("purge")}
                >
                  <Trash2 size={15} />
                  Delete permanently
                </Button>
              </>
            ) : (
              <>
                <Button onClick={() => setMove(true)}>
                  <FolderInput size={15} />
                  Move
                </Button>
                <Menu
                  items={[
                    {
                      label: "Add tag",
                      action: () => void editTags("add_tag"),
                    },
                    {
                      label: "Remove tag",
                      action: () => void editTags("remove_tag"),
                    },
                  ]}
                >
                  <Tag size={15} />
                  Tags
                </Menu>
                <Menu
                  items={[
                    { label: "Suspend", action: () => void bulk("suspend") },
                    {
                      label: "Unsuspend",
                      action: () => void bulk("unsuspend"),
                    },
                    {
                      label: "Bury until tomorrow",
                      action: () => void bulk("bury"),
                      separator: true,
                    },
                    { label: "Unbury", action: () => void bulk("unbury") },
                    {
                      label: "Set due date",
                      action: () => void setDue(),
                      separator: true,
                    },
                    {
                      label: "Reschedule",
                      action: () => void destructive("reschedule"),
                    },
                    {
                      label: "Reset to New",
                      action: () => void destructive("reset"),
                      danger: true,
                    },
                  ]}
                >
                  <SlidersHorizontal size={15} />
                  Scheduling
                </Menu>
                <Button
                  onClick={() => exportCards(undefined, Array.from(selected))}
                >
                  <Download size={15} />
                </Button>
                <Button
                  variant="danger"
                  aria-label="Delete selected notes"
                  onClick={() => void destructive("delete")}
                >
                  <Trash2 size={15} />
                </Button>
              </>
            )}
          </div>
        ) : (
          !trash && (
            <Button
              variant="ghost"
              onClick={() => void navigate({ page: "browse", trash: true })}
            >
              <Trash2 size={15} />
              Recently Deleted
            </Button>
          )
        )}
      </div>
      {query.error ? (
        <ErrorPanel
          message={errorMessage(query.error)}
          retry={() => void query.refetch()}
        />
      ) : query.isPending ? (
        <Loading />
      ) : !cards.length ? (
        <Empty title={trash ? "Nothing here to recover" : "No cards found"}>
          {trash
            ? "Deleted notes will appear here until you restore or permanently remove them."
            : "Try another search or add a card to your collection."}
        </Empty>
      ) : (
        <div
          className="browser-table"
          ref={scroll}
          role="table"
          aria-label="Cards"
        >
          <div
            className="table-header"
            role="row"
            style={{ gridTemplateColumns: widths }}
          >
            {table.getHeaderGroups()[0].headers.map((header) => (
              <div key={header.id} role="columnheader">
                {[
                  "front",
                  "deck",
                  "state",
                  "due",
                  "reviews",
                  "lapses",
                ].includes(header.id) ? (
                  <button onClick={() => sortBy(header.id)}>
                    {flexRender(
                      header.column.columnDef.header,
                      header.getContext(),
                    )}
                    <ArrowUpDown
                      size={12}
                      className={sort === header.id ? "text-violet" : ""}
                    />
                  </button>
                ) : (
                  flexRender(
                    header.column.columnDef.header,
                    header.getContext(),
                  )
                )}
              </div>
            ))}
          </div>
          <div
            style={{
              height: virtual.getTotalSize(),
              position: "relative",
              width: table.getTotalSize(),
            }}
          >
            {virtual.getVirtualItems().map((item) => {
              const row = rows[item.index];
              return (
                <div
                  key={row.id}
                  role="row"
                  className={`table-row ${selected.has(row.original.id) ? "selected" : ""}`}
                  style={{
                    gridTemplateColumns: widths,
                    height: item.size,
                    transform: `translateY(${item.start}px)`,
                  }}
                  onDoubleClick={() =>
                    !trash &&
                    void navigate({ page: "editor", cardId: row.original.id })
                  }
                >
                  {row.getVisibleCells().map((cell) => (
                    <div role="cell" key={cell.id}>
                      {flexRender(
                        cell.column.columnDef.cell,
                        cell.getContext(),
                      )}
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        </div>
      )}
      <footer className="browser-pagination">
        <span>
          {query.data?.total
            ? `${offset + 1}–${Math.min(offset + 100, query.data.total)} of ${number(query.data.total)}`
            : "0 cards"}
        </span>
        <div className="inline">
          <Button
            disabled={!offset}
            onClick={() => {
              setOffset((n) => Math.max(0, n - 100));
              scroll.current?.scrollTo(0, 0);
            }}
          >
            Previous
          </Button>
          <Button
            disabled={offset + 100 >= (query.data?.total ?? 0)}
            onClick={() => {
              setOffset((n) => n + 100);
              scroll.current?.scrollTo(0, 0);
            }}
          >
            Next
          </Button>
        </div>
      </footer>
      {detail && (
        <CardDetails cardId={detail} onClose={() => setDetail(null)} />
      )}{" "}
      {move && (
        <Modal
          title={trash ? "Restore notes" : "Move cards"}
          description="Scheduling and review history are preserved."
          open
          onClose={() => setMove(false)}
        >
          <label className="field">
            <span>Destination deck</span>
            <select
              value={destination}
              onChange={(e) => setDestination(e.target.value)}
            >
              {data.decks.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.path}
                </option>
              ))}
            </select>
          </label>
          <div className="modal-actions">
            <Button onClick={() => setMove(false)}>Cancel</Button>
            <Button
              variant="primary"
              disabled={!destination}
              onClick={async () => {
                await bulk(trash ? "restore" : "move", destination);
                setMove(false);
              }}
            >
              {trash ? "Restore notes" : "Move cards"}
            </Button>
          </div>
        </Modal>
      )}
      {tagManager && <TagManager onClose={() => setTagManager(false)} />}
    </div>
  );
}
function TagManager({ onClose }: { onClose: () => void }) {
  const { data, run, ask, confirm } = useTala();
  async function rename(from: string) {
    const to = await ask("Rename tag", "New tag name", from);
    if (to)
      await run(
        () => rpc({ action: "edit_tag", payload: { from, to } }),
        "Tag renamed",
      );
  }
  async function remove(from: string) {
    if (
      await confirm({
        title: `Delete tag “${from}”?`,
        message:
          "The tag will be removed from every note. Notes, cards, and schedules are unchanged.",
        confirm: "Delete tag",
        danger: true,
      })
    )
      await run(
        () => rpc({ action: "edit_tag", payload: { from, to: null } }),
        "Tag deleted",
      );
  }
  return (
    <Modal
      title="Manage tags"
      description="Tags belong to notes. Renaming a tag updates every note that uses it."
      open
      onClose={onClose}
    >
      <div className="tag-manager">
        {data.tags.length ? (
          data.tags.map((tag) => (
            <div key={tag}>
              <span className="tag">{tag}</span>
              <div className="inline">
                <Button
                  variant="ghost"
                  aria-label={`Rename ${tag}`}
                  onClick={() => void rename(tag)}
                >
                  <Pencil size={15} />
                </Button>
                <Button
                  variant="ghost"
                  aria-label={`Delete tag ${tag}`}
                  onClick={() => void remove(tag)}
                >
                  <Trash2 size={15} />
                </Button>
              </div>
            </div>
          ))
        ) : (
          <p className="muted">Add tags while creating or editing cards.</p>
        )}
      </div>
    </Modal>
  );
}
