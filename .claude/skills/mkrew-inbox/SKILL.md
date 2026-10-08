---
name: mkrew-inbox
description: Use when the user asks to scan, triage or plan the inbox ("scan inbox", "plan the inbox", "/mkrew-inbox"), or says they added bugs, enhancements or feature requests to docs/INBOX.md.
---

# Mkrew: plan the inbox

Turn new items in `docs/INBOX.md` into one implementation plan, then stop for the user's review.
This skill plans only. It never implements and never commits.

## Steps

1. **Collect.** Read `docs/INBOX.md`. Take only `- [ ]` lines under `## Items`, plus their
   indented lines. Skip `[~]`, `[x]`, `[-]`. No `[ ]` items → say "Inbox empty", stop.
2. **Classify** each item: `bug`, `enh` (change existing behavior) or `feat` (new behavior).
   Keep the user's tag if given.
3. **Investigate (read-only).** For each item find the code and SPEC sections involved
   (grep SPEC headings, do not read it whole). Bug → find the root cause with
   systematic-debugging; name the file and function. Several items → one `scout` subagent per
   item, in parallel.
4. **Decide open points** using SPEC §2. A UI/design choice (e.g. where a bar moves) → put 2–3
   options with a recommendation in that task and list it under `## Open questions`.
   Deviates from SPEC → the task includes the ADR + SPEC update.
5. **Write the plan** with the writing-plans skill to
   `docs/superpowers/plans/YYYY-MM-DD-inbox-<slug>.md`. One task per item (merge items only if
   they touch the same code). Copy `## Global Constraints` from the newest plan in that folder.
   Each task names its inbox line, so the implementer can tick it.
6. **Mark items.** Change each planned item `- [ ]` → `- [~]` and append
   ` → plan: <plan file> Task <n>`. Do not touch other items.
7. **Stop.** Report a table: item · type · task · open question (if any). Do not start
   implementation; the user reviews the plan first.

## When implementing a plan later

After a task passes verification, change its inbox line `[~]` → `[x]` in the same change.
An item the user rejects → `[-]`.

## Common mistakes

| Mistake | Fix |
|---|---|
| Re-planning `[~]`/`[x]` items | Only `[ ]` lines are input |
| Fixing the symptom named in the report | Plan the root cause; grep every caller |
| Starting to code after the plan | Stop at step 7 |
| Committing | User preference: leave changes uncommitted |
