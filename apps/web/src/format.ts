// The desktop app's wording (apps/desktop/src/ui/format.rs), so both say the same thing.

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

const round = (ms: number, unit: number) => Math.floor((ms + unit / 2) / unit);

/** Rounded the way the desktop rounds: hours up to 23½, then days (1 is "tomorrow") up to 60, then months. */
function distance(ms: number): { n: number; unit: string } | "tomorrow" {
  if (ms < HOUR) return { n: round(ms, MINUTE), unit: "minute" };
  if (ms < DAY - HOUR / 2) return { n: round(ms, HOUR), unit: "hour" };
  if (ms < 60 * DAY) {
    const days = round(ms, DAY);
    return days === 1 ? "tomorrow" : { n: days, unit: "day" };
  }
  return { n: Math.floor((ms + 15 * DAY) / (30 * DAY)), unit: "month" };
}

/** "in 10 minutes", "in 3 hours", "tomorrow", "in 4 days", "in 2 months". */
export function until(at: number, now = Date.now()): string {
  const ms = Math.max(0, at - now);
  if (ms < MINUTE) return "in under a minute";
  const d = distance(ms);
  return d === "tomorrow" ? d : `in ${plural(d.n, d.unit)}`;
}

/** "12 min", "3 hours", "tomorrow", "2 days", "3 months". */
export function untilShort(at: number, now = Date.now()): string {
  const ms = Math.max(0, at - now);
  if (ms < HOUR) return `${Math.max(1, round(ms, MINUTE))} min`;
  const d = distance(ms);
  return d === "tomorrow" ? d : plural(d.n, d.unit);
}

export function ago(at: number, now = Date.now()): string {
  const ms = Math.max(0, now - at);
  if (ms < MINUTE) return "just now";
  if (ms < HOUR) return `${Math.floor(ms / MINUTE)} min ago`;
  if (ms < DAY) return `${Math.floor(ms / HOUR)} h ago`;
  return `${Math.floor(ms / DAY)} d ago`;
}

export function counts(due: number, fresh: number): string {
  if (due === 0 && fresh === 0) return "Nothing due";
  if (fresh === 0) return `${due} due`;
  if (due === 0) return `${fresh} new`;
  return `${due} due · ${fresh} new`;
}

export const hiddenSides = (n: number) => plural(n, "hidden side");
export const reviews = (n: number) => plural(n, "review");
export const lapses = (n: number) => plural(n, "lapse");

/** Markdown flattened to one line of plain text, for compact items and sizing tiles. */
export function plain(markdown: string): string {
  return markdown
    .replace(/```[^\n]*\n?/g, "")
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}(#{1,6}\s+|>\s?|[-*+]\s+(\[[ xX]\]\s+)?|\d+[.)]\s+)/gm, "")
    .replace(/(\*\*|__|\*|_|~~|`)/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** How big a tile's front is, by its plain length (apps/desktop/src/ui/cards.rs). */
export function frontSize(text: string): "xl" | "l" | "m" | "s" {
  const n = plain(text).length;
  if (n <= 24) return "xl";
  if (n <= 60) return "l";
  if (n <= 140) return "m";
  return "s";
}
