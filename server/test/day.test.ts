import { describe, expect, test } from "bun:test";
import { dayBounds, zoneOf } from "../src/Day.ts";

const DAY = 86_400_000;
const HOUR = 3_600_000;

// Expected values computed with Python's zoneinfo: the start and end of the review day containing each instant.
const CASES: Array<[now: number, zone: string, rollover: number, start: number, end: number]> = [
  [Date.parse("2026-01-15T09:30:00Z"), "UTC", 4, Date.parse("2026-01-15T04:00:00Z"), Date.parse("2026-01-16T04:00:00Z")],
  [Date.parse("2026-01-15T09:30:00Z"), "UTC", 0, Date.parse("2026-01-15T00:00:00Z"), Date.parse("2026-01-16T00:00:00Z")],
  // Before the rollover, the day is still yesterday's.
  [Date.parse("2026-01-15T03:00:00Z"), "UTC", 4, Date.parse("2026-01-14T04:00:00Z"), Date.parse("2026-01-15T04:00:00Z")],
  [
    Date.parse("2026-01-15T09:30:00Z"),
    "America/New_York",
    4,
    Date.parse("2026-01-15T09:00:00Z"),
    Date.parse("2026-01-16T09:00:00Z"),
  ],
  [
    Date.parse("2026-01-15T07:00:00Z"),
    "America/New_York",
    4,
    Date.parse("2026-01-14T09:00:00Z"),
    Date.parse("2026-01-15T09:00:00Z"),
  ],
  [
    Date.parse("2026-01-15T20:00:00Z"),
    "Asia/Shanghai",
    4,
    Date.parse("2026-01-15T20:00:00Z"),
    Date.parse("2026-01-16T20:00:00Z"),
  ],
  // Spring forward (US, 2026-03-08): the day is 23 hours.
  [
    Date.parse("2026-03-08T06:00:00Z"),
    "America/New_York",
    4,
    Date.parse("2026-03-07T09:00:00Z"),
    Date.parse("2026-03-08T08:00:00Z"),
  ],
  [
    Date.parse("2026-03-08T12:00:00Z"),
    "America/New_York",
    4,
    Date.parse("2026-03-08T08:00:00Z"),
    Date.parse("2026-03-09T08:00:00Z"),
  ],
  // Fall back (US, 2026-11-01): the day is 25 hours.
  [
    Date.parse("2026-11-01T08:00:00Z"),
    "America/New_York",
    4,
    Date.parse("2026-10-31T08:00:00Z"),
    Date.parse("2026-11-01T09:00:00Z"),
  ],
  [
    Date.parse("2026-11-01T12:00:00Z"),
    "America/New_York",
    4,
    Date.parse("2026-11-01T09:00:00Z"),
    Date.parse("2026-11-02T09:00:00Z"),
  ],
  // Europe's transitions, an offset that isn't a whole hour, and a quarter-hour one.
  [
    Date.parse("2026-03-29T06:00:00Z"),
    "Europe/Berlin",
    4,
    Date.parse("2026-03-29T02:00:00Z"),
    Date.parse("2026-03-30T02:00:00Z"),
  ],
  [
    Date.parse("2026-10-25T06:00:00Z"),
    "Europe/Berlin",
    4,
    Date.parse("2026-10-25T03:00:00Z"),
    Date.parse("2026-10-26T03:00:00Z"),
  ],
  [
    Date.parse("2026-06-01T16:30:00Z"),
    "Pacific/Chatham",
    4,
    Date.parse("2026-06-01T15:15:00Z"),
    Date.parse("2026-06-02T15:15:00Z"),
  ],
  [
    Date.parse("2026-06-01T02:00:00Z"),
    "Asia/Kolkata",
    4,
    Date.parse("2026-05-31T22:30:00Z"),
    Date.parse("2026-06-01T22:30:00Z"),
  ],
];

describe("the review day", () => {
  test.each(CASES)("bounds for %s in %s, rollover %d", (now, zone, rollover, start, end) => {
    expect(dayBounds(now, zone, rollover)).toEqual([start, end]);
  });

  test("a day always contains its instant, and is 23-25 hours", () => {
    for (const [now, zone, rollover] of CASES) {
      const [start, end] = dayBounds(now, zone, rollover);
      expect(start).toBeLessThanOrEqual(now);
      expect(now).toBeLessThan(end);
      expect(end - start).toBeGreaterThanOrEqual(23 * HOUR);
      expect(end - start).toBeLessThanOrEqual(25 * HOUR);
    }
  });

  test("the zone header is taken as given, or UTC", () => {
    expect(zoneOf(undefined)).toBe("UTC");
    expect(zoneOf("")).toBe("UTC");
    expect(zoneOf(" America/New_York ")).toBe("America/New_York");
    expect(zoneOf("Not/AZone")).toBe("UTC");
    expect(zoneOf("A".repeat(100))).toBe("UTC");
    expect(zoneOf("Pacific/Chatham")).toBe("Pacific/Chatham");
  });
});
