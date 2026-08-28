import { createContext, useContext, type ReactNode } from "react";
import type { Bootstrap } from "../bindings/Bootstrap";
import type { Deck } from "../bindings/Deck";
export type Route =
  | { page: "today" | "decks" | "statistics" | "settings" | "study" }
  | { page: "deck"; id: string }
  | { page: "editor"; cardId?: string; deckId?: string }
  | { page: "browse"; deckId?: string; trash?: boolean; tag?: string };
export type ConfirmOptions = {
  title: string;
  message: string;
  confirm?: string;
  danger?: boolean;
};
export interface TalaContextValue {
  data: Bootstrap;
  route: Route;
  navigate: (route: Route) => Promise<void>;
  /** Refreshes cached views after success; undefined signals a handled error, null may be success. */
  run: <T>(
    operation: () => Promise<T>,
    success?: string,
  ) => Promise<T | undefined>;
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  ask: (title: string, label: string, value?: string) => Promise<string | null>;
  notify: (message: string, error?: boolean) => void;
  editDeck: (deck?: Deck) => void;
  startStudy: (deckId?: string) => Promise<void>;
  setDirty: (dirty: boolean) => void;
  importCards: (deckId?: string) => void;
  exportCards: (deckId?: string, ids?: string[]) => void;
}
export const TalaContext = createContext<TalaContextValue | null>(null);
export function useTala() {
  const context = useContext(TalaContext);
  if (!context) throw new Error("Tala context is missing");
  return context;
}
export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
      {hint && <small>{hint}</small>}
    </label>
  );
}
