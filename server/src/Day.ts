// The caller's review day (docs/contract.md, "Scheduling"): their IANA zone from `X-Furiten-Timezone` (UTC without
// one) and the library's rollover hour, the local hour the day starts at. A card is due for the whole day its `due`
// falls on, so a day's reviews are all there from the moment the day starts.

const HOUR = 3_600_000;
const DAY = 86_400_000;

const parts = (t: number, zone: string) => {
  const out: Record<string, number> = {};
  for (const p of new Intl.DateTimeFormat("en-US", {
    timeZone: zone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).formatToParts(t)) {
    if (p.type !== "literal") out[p.type] = Number(p.value);
  }
  return out;
};

/** The local wall time of instant `t` in `zone`, as the same clock time read as UTC. */
function wall(t: number, zone: string): number {
  const p = parts(t, zone);
  // `hour` can be 24 at midnight (hourCycle h24), which reads as the next day's start.
  return Date.UTC(p.year!, p.month! - 1, p.day!, p.hour! % 24, p.minute!, p.second!);
}

/** The zone's offset from UTC at instant `t`, in ms east (New York is negative). */
const zoneOffset = (t: number, zone: string): number => wall(t, zone) - t;

const mod = (n: number, m: number) => ((n % m) + m) % m;

/**
 * The current review day's start and end (the next rollover), in Unix ms. A timestamp `t` falls on the local
 * calendar date of `t - rollover` hours: with a 4am rollover, 2am on the 15th belongs to the 14th's day.
 */
export function dayBounds(now: number, zone: string, rollover: number): [number, number] {
  const h = rollover * HOUR;
  // All date arithmetic happens in wall time, where a day is a clean 24 hours; only the final conversion to an
  // instant sees the zone's offsets, so DST shifts the boundary by an hour at most, on the night it changes.
  const midnight = wall(now, zone) - h - mod(wall(now, zone) - h, DAY);
  // A wall time W belongs to the instant t with wall(t) = W, so t = W - zoneOffset(t); a second pass lands on it.
  const instant = (w: number): number => {
    let t = w - zoneOffset(now, zone);
    return w - zoneOffset(t, zone);
  };
  return [instant(midnight + h), instant(midnight + DAY + h)];
}

const known = new Set<string>(["UTC"]);

/** The zone a request named in `X-Furiten-Timezone`, or UTC: absent, oversized or unknown names are ignored. */
export function zoneOf(header: string | undefined): string {
  const zone = (header ?? "").trim();
  if (zone !== "" && zone.length <= 64 && !known.has(zone)) {
    try {
      new Intl.DateTimeFormat("en-US", { timeZone: zone }).format();
      known.add(zone);
    } catch {
      // Not a zone this runtime knows; the request is answered in UTC.
    }
  }
  return known.has(zone) ? zone : "UTC";
}
