import type { CardView } from "../bindings/CardView";
export const number = (n: number) => new Intl.NumberFormat().format(n);
export const percent = (n: number) => `${Math.round(n * 100)}%`;
export function duration(seconds: number) {
  if (seconds < 60) return `${Math.round(seconds)} sec`;
  if (seconds < 3600) return `${Math.round(seconds / 60)} min`;
  return `${Math.floor(seconds / 3600)}h ${Math.round((seconds % 3600) / 60)}m`;
}
export function dateTime(timestamp: number | null, short = false) {
  if (timestamp == null) return "Not yet reviewed";
  return new Date(timestamp * 1000).toLocaleString(
    undefined,
    short
      ? { month: "short", day: "numeric" }
      : {
          month: "short",
          day: "numeric",
          year: "numeric",
          hour: "numeric",
          minute: "2-digit",
        },
  );
}
export function relativeDue(timestamp: number) {
  const diff = timestamp * 1000 - Date.now();
  if (diff <= 0) return "Due now";
  if (diff < 60_000) return "Less than a minute";
  if (diff < 3_600_000) return `In ${Math.ceil(diff / 60_000)} min`;
  if (diff < 86_400_000) return `In ${Math.ceil(diff / 3_600_000)} hours`;
  return dateTime(timestamp, true);
}
export const behaviorLabel = (behavior: string) =>
  ({ normal: "Normal", reversed: "Reversed", typed: "Type in the Answer" })[
    behavior
  ] ?? behavior;
export function cardState(card: CardView) {
  return card.suspended
    ? "suspended"
    : card.buriedUntil && card.buriedUntil > Date.now() / 1000
      ? "buried"
      : card.schedule.phase;
}
export function isTyping(target: EventTarget | null) {
  return (
    target instanceof HTMLElement &&
    !!target.closest(
      'input,textarea,select,[contenteditable="true"],[role="dialog"]',
    )
  );
}
