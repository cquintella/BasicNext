# Catalog pass: typed facts for the remaining free-text identities

Cut from bucket 0.6.2b R7b-2 (2026-09-28). Every catalog row already has a
title, a cause, and help (gate `catalog_rows_have_title_cause_and_help`).
What remains is cosmetic:

- About 20 identities have one producer with one fixed sentence
  (`INVALID_FOR_STEP`, `MISSING_RETURN`, `IMPORTED_START`, …): make each a
  fixed template with no facts, which removes the title/message repetition
  ("FOR STEP is zero: FOR STEP cannot be zero"). This breaks catalog
  overlays that use `{$message}` for them: say so in the release notes.
- The multi-producer identities on `LEGACY_MESSAGE` / `DETAIL` (63 + 29 at
  the cut) need a schema each, designed from their producers
  (`STALE_HANDLE` 21 producers, `HOST_CAPABILITY_UNAVAILABLE` 18,
  `INVALID_IR` 24, …).
