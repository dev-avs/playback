import { test } from "node:test";
import assert from "node:assert/strict";
import { parseLrc } from "../src/lyrics.ts";

test("a line with several timestamps is sung at each of them", () => {
  const lines = parseLrc("[00:12.00][00:45.50]again\n[00:20.00]between");
  assert.deepEqual(
    lines.map((l) => [l.t, l.text]),
    [
      [12, "again"],
      [20, "between"],
      [45.5, "again"],
    ],
  );
});

test("a single timestamp still parses, and an empty line seals the one before", () => {
  const lines = parseLrc("[00:01.00]one\n[00:03.00]\n[00:10.00]two");
  assert.deepEqual(
    lines.map((l) => [l.t, l.text, l.end]),
    [
      [1, "one", 3],
      [10, "two", undefined],
    ],
  );
});
