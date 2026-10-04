# Human checks

The build loop never stops to ask you. Everything that needs your eyes, hands or decision
lands here. Skim it at the end of each milestone (or when `scripts/status.sh` says so).

How to answer: write your answer under the item, move it to "Done", and if the loop status is
`WAITING_HUMAN` set `status: RUNNING` in `docs/PROGRESS.md`. Then restart the loop.

## Waiting (blocking)

_Items that stop the loop until you answer._

## To verify (non-blocking)

_Things the agent could not verify automatically (visual checks, real agents, Windows-only behavior).
The loop keeps going; report problems by adding a task line to PROGRESS.md or writing them here._

## Questions with assumed defaults

_Decisions the agent made on its own. Override them here if you disagree._

## Done

