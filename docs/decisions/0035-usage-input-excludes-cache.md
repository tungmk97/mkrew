# 0035 — `session.usage` input excludes the cache

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10

## Context

SPEC §20E.3 sums "input/output tokens" and does not say whether the prompt cache counts. The code
(T13.9, HUMAN_CHECKS) made `input` = every token the model read: Claude `input_tokens` + cache
creation + cache read, Codex `input_tokens` (which already contains `cached_input_tokens`). The
inbox wants cache read and cache write shown on their own (status bar, right panel) and the model
name, and omp joins with its own counts. What the three agents write natively:

| Agent | fresh input | cache read | cache write | `input` in the file |
|---|---|---|---|---|
| Claude | `input_tokens` | `cache_read_input_tokens` | `cache_creation_input_tokens` | fresh only |
| omp | `input` | `cacheRead` | `cacheWrite` | fresh only (`totalTokens` = all four) |
| Codex | `input_tokens − cached_input_tokens` | `cached_input_tokens` | none | includes the cache |

## Options

1. Keep `input` = everything read; add `cache_read` / `cache_write` as subsets of it. Old stored
   events stay correct. But a consumer must subtract to get fresh input, omp and Claude must be
   ADDED up to match, and `input + cache_read + …` double-counts if a consumer forgets.
2. `input` = fresh input only; `cache_read` and `cache_write` beside it; all four add up to every
   token. Old stored events keep the old `input` (cache inside, cache fields 0) until the session's
   next turn end re-reads the transcript.
3. A new field `fresh_input`, leaving `input` as it is. No mixed meaning, but two fields say
   "input" and every format still needs the arithmetic.

## Decision

Option 2. It is what Claude and omp write natively and what the item asks for ("split cache out of
`input`"); only Codex needs one subtraction, done once in the reader. The four counts add up, so
"total tokens" is a sum with no subset rule (`Usages::total` = input + cache_read + cache_write +
output, the same number as before for Claude and Codex, so card and sidebar totals do not move).
The context size is untouched (Claude: input + cache write + cache read of the newest main-thread
message; Codex: `last_token_usage.total_tokens`), so `ctx %` does not change.
`PROTOCOL_VERSION` goes to 33; the new fields are `#[serde(default)]`, so events stored by an
older daemon still load (`session.usage` replays them).

## Consequences

- SPEC §20E.3, its protocol note and the §7 event table change in the same commit; the T13.9 note
  in HUMAN_CHECKS is marked as changed.
- Until a session's next turn end after the upgrade, its stored event has the old meaning: the
  hover of `ctx` shows the whole input as "in" and no "cached" part; totals and ctx % are right.
  The daemon restart that the protocol bump needs makes the first turn end re-read from the start.
- Sub-agent (Claude sidechain) tokens stay in the totals but not in the context, as before.
- Still no money estimate (SPEC §20E.3, §1).
