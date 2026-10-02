import type { Deck } from "../bindings/Deck";

export function branchIds(decks: Deck[], id: string): Set<string> {
  const ids = new Set<string>();
  const pending = [id];
  while (pending.length) {
    const current = pending.pop()!;
    if (ids.has(current)) continue;
    ids.add(current);
    pending.push(
      ...decks
        .filter((deck) => deck.parentId === current)
        .map((deck) => deck.id),
    );
  }
  return ids;
}

export function deckAncestors(decks: Deck[], deck: Deck): Deck[] {
  const chain: Deck[] = [];
  const visited = new Set<string>([deck.id]);
  let parent = deck.parentId;
  while (parent && !visited.has(parent)) {
    visited.add(parent);
    const next = decks.find((deck) => deck.id === parent);
    if (!next) break;
    chain.unshift(next);
    parent = next.parentId;
  }
  return chain;
}
