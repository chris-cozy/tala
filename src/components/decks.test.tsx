import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Deck } from "../bindings/Deck";
import type { TalaContextValue } from "../lib/context";
import { TalaContext } from "../lib/context";

vi.mock("../pages/Decks", () => ({
  DeckMenu: () => <button type="button" aria-label="Deck options" />,
}));

import { DeckTree } from "./DeckTree";

const emptyCounts = {
  total: 0,
  newCount: 0,
  learning: 0,
  review: 0,
  due: 0,
  inReview: 0,
  limitedNew: 0,
  limitedReview: 0,
};

function deck(
  id: string,
  name: string,
  parentId: string | null,
  path: string,
  total: number,
  due: number,
): Deck {
  return {
    id,
    name,
    parentId,
    path,
    subtreeCounts: { ...emptyCounts, total, due },
    cover: null,
    color: "teal",
    settings: {
      newLimit: 20,
      reviewLimit: 200,
      retention: 0.9,
      learningSteps: [60, 600],
      relearningSteps: [600],
      maximumInterval: 36500,
      newOrder: "created",
      reviewOrder: "due",
      newPlacement: "after",
      leechThreshold: 8,
      suspendLeeches: false,
    },
    createdAt: 1,
    updatedAt: 1,
    total,
    newCount: due,
    learning: 0,
    review: 0,
    due,
    inReview: 0,
    limitedNew: 0,
    limitedReview: 0,
  };
}

const root = deck("root", "Science", null, "Science", 2, 5);
const child = deck("child", "Physics", "root", "Science::Physics", 2, 3);
const leaf = deck("leaf", "Motion", "child", "Science::Physics::Motion", 0, 0);

function context(overrides: Partial<TalaContextValue> = {}) {
  return {
    data: { decks: [root, child, leaf] },
    route: { page: "decks" },
    navigate: vi.fn(async () => {}),
    run: vi.fn(async <T,>(operation: () => Promise<T>) => operation()),
    confirm: vi.fn(async () => true),
    ask: vi.fn(async () => null),
    notify: vi.fn(),
    editDeck: vi.fn(),
    startStudy: vi.fn(async () => {}),
    setDirty: vi.fn(),
    importCards: vi.fn(),
    exportCards: vi.fn(),
    ...overrides,
  } as unknown as TalaContextValue;
}

describe("deck hierarchy navigation", () => {
  beforeEach(() => localStorage.clear());
  afterEach(cleanup);

  it("expands branches, reports subtree totals, and starts scoped study", () => {
    const value = context();
    render(
      <TalaContext.Provider value={value}>
        <DeckTree />
      </TalaContext.Provider>,
    );

    const science = screen.getByRole("treeitem", { name: "Science" });
    expect(science).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("treeitem", { name: "Physics" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Expand Science" }));
    expect(
      screen.getByRole("treeitem", { name: "Physics" }),
    ).toBeInTheDocument();
    expect(science).toHaveTextContent("2 cards · 1 subdeck");
    fireEvent.click(screen.getByRole("button", { name: "Study Science" }));
    expect(value.startStudy).toHaveBeenCalledWith("root");
  });

  it("opens matching ancestors while searching and opens the selected deck", () => {
    const value = context();
    render(
      <TalaContext.Provider value={value}>
        <DeckTree search="motion" />
      </TalaContext.Provider>,
    );
    const science = screen
      .getAllByRole("treeitem")
      .find((item) => item.getAttribute("aria-label") === "Science");
    expect(science).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(
      screen.getByRole("button", { name: "Open Science::Physics::Motion" }),
    );
    expect(value.navigate).toHaveBeenCalledWith({ page: "deck", id: "leaf" });
    expect(
      screen.getByRole("button", { name: "Study Science::Physics::Motion" }),
    ).toBeDisabled();
  });
});
