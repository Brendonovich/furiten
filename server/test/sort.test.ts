import { expect, test } from "bun:test";
import { fnv1a, naturalCompare, titleKey } from "../src/Sort.ts";

// The same cases as crates/core/src/sort/tests.rs, so both implementations order cards alike.
test("names compare naturally without Markdown marks", () => {
  expect(naturalCompare("a2", "a10")).toBeLessThan(0);
  expect(naturalCompare("a010", "a9")).toBeGreaterThan(0);
  expect(naturalCompare("a", "a1")).toBeLessThan(0);
  expect(naturalCompare("b", "a9")).toBeGreaterThan(0);
  expect(naturalCompare("x0", "x00")).toBe(0);
  expect(titleKey("\n> **La** `biblioteca`")).toBe("la biblioteca");
  expect(titleKey("- banana")).toBe("banana");
});

test("random uses the core's hash", () => {
  expect(fnv1a("7:c00")).toBe(0x0037724d);
});
