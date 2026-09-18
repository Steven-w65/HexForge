# Task 7 Report: Hex Input, Geometry, Selection, and Virtual Scrolling

## Scope

Implemented pure TypeScript modules under `src/hex` with no Vue, backend, filesystem, or rendering dependencies:

- `input.ts`: strict decimal/hex offset parsing with bigint bounds, byte-sequence parsing, and single-byte parsing.
- `selection.ts`: inclusive bigint selection normalization and containment.
- `layout.ts`: deterministic row metrics, grouped hex/ASCII columns, bigint-aware visible ranges, and hit testing.
- `virtualScroll.ts`: bigint-safe integer row/track mapping with endpoint and degenerate-case clamping.

## TDD evidence

RED phase:

```text
npm test -- src/hex/input.test.ts src/hex/layout.test.ts src/hex/selection.test.ts src/hex/virtualScroll.test.ts
4 failed suites; imports could not resolve because input.ts, layout.ts, selection.ts, and virtualScroll.ts did not exist.
```

GREEN phase:

```text
npm test -- src/hex
4 passed files, 42 passed tests

npm run typecheck
vue-tsc --noEmit (passed)

npm test
6 passed files, 47 passed tests
```

## Test coverage

- Anchored decimal, `0x`, and `0X` offsets; malformed, signed, whitespace-only, empty-file, equal-to-size, and >`Number.MAX_SAFE_INTEGER` offsets.
- Complete byte tokens, ASCII whitespace, leading/trailing whitespace, malformed tokens, sequence preservation, and single-byte edit validation.
- Forward/reverse/single/large-bigint selections and inclusive containment.
- Hex/ASCII same-byte hit testing, headers/outside/grouping gaps, short final rows, empty files, 16/32-byte layouts, prefetching, and 1 MiB read cap.
- 64-bit virtual-scroll endpoint exactness, clamping, zero/one-row and zero-track behavior, and midpoint monotonicity beyond safe integer precision.

## Self-review

- All file offsets and row-derived byte positions remain bigint until a bounded byte length is converted to `number`.
- Input regular expressions are anchored, reject signs/malformed literals, and use `BigInt` for offset conversion.
- Layout grouping gaps are treated as non-hit regions; both hex and ASCII cells map to the same byte offset.
- Visible ranges clamp to file bounds and the one-mebibyte read limit; empty files return an empty range.
- Virtual-scroll ratios use bigint multiplication/division and clamp endpoints before converting the final pixel result to `number`.
- No Vue components, backend calls, rendering code, or unrelated files were changed.

## Review round 1 fixes

Root causes were reproduced with focused regressions before changing production code:

- `groupGap` was incorrectly derived from `measuredCharWidth`, so a 10px measured font produced a 10px grouping gap instead of the fixed 8px contract.
- `visibleRange` accepted unsafe numeric scroll rows and converted them after rounding; a huge viewport also allowed an unbounded bigint-to-number row count conversion.
- `hitTestByte` multiplied the CSS row index by bytes-per-row as a `number` before converting to bigint.

The fixes add a fixed `GROUP_GAP = 8`, make `visibleRange` bigint-scroll-row-only with runtime validation, cap represented rows at `floor(1MiB / bytesPerRow)` before conversion, and use a safe CSS row check followed by bigint multiplication/addition in hit testing.

Review RED evidence:

```text
npm test -- src/hex/layout.test.ts
8 tests, 2 failed: measured-width group gap returned 10 instead of 8; numeric scroll row was not rejected.
```

Review GREEN evidence:

```text
npm test -- src/hex
4 passed files, 45 passed tests

npm run typecheck
vue-tsc --noEmit (passed)

npm test
6 passed files, 50 passed tests
```
