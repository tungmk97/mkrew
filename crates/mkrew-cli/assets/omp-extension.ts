// Installed by `mk integration install omp` (Mkrew). Reinstalling overwrites this file;
// `mk integration uninstall omp` deletes it. Put your own extensions beside it.
// @ts-nocheck
import { execFile } from "node:child_process";

// Only inside a Mkrew pane, and never for an `omp` started by omp's own shell (OMPCODE=1):
// that nested session would report its id over the pane's root session.
const enabled = !!process.env.MKREW_SESSION_ID && process.env.OMPCODE !== "1";

let queue = Promise.resolve();

// The daemon's own `mk` (MKREW_MK, set in every Mkrew pane), so a stale `mk` on PATH never
// answers. One `mk hook omp <event> <json>` at a time, in order; `mk` logs its own failures
// to <data>/logs/hook.log, so errors are ignored here.
const mk = process.env.MKREW_MK || "mk";

function report(event, ctx) {
  let sessionId;
  try {
    const id = ctx?.sessionManager?.getSessionId?.();
    sessionId = typeof id === "string" && id.length > 0 ? id : undefined;
  } catch {}
  const payload = JSON.stringify(sessionId ? { session_id: sessionId } : {});
  queue = queue.then(
    () =>
      new Promise((done) => {
        execFile(mk, ["hook", "omp", event, payload], { windowsHide: true, timeout: 10000 }, () => done());
      }),
  );
}

export default function (pi) {
  if (!enabled) return;
  // The pane's root session is the one with a UI; task subagents have none.
  let root = false;
  let waiting = 0;
  const on = (name, handler) =>
    pi.on(name, (event, ctx) => {
      if (!root) {
        if (ctx?.hasUI !== true) return;
        root = true;
      }
      handler(event, ctx);
    });
  const ask = (ctx) => {
    waiting += 1;
    report("needs_input", ctx);
  };
  const answered = (ctx) => {
    waiting = Math.max(0, waiting - 1);
    if (waiting === 0) report("busy", ctx);
  };

  on("session_start", (_event, ctx) => report("session_start", ctx));
  on("session_switch", (_event, ctx) => report("session_start", ctx));
  on("agent_start", (_event, ctx) => report("busy", ctx));
  on("agent_end", (event, ctx) => {
    if (event?.willContinue !== true) report("done", ctx);
  });
  on("tool_approval_requested", (_event, ctx) => ask(ctx));
  on("tool_approval_resolved", (_event, ctx) => answered(ctx));
  on("tool_execution_start", (event, ctx) => {
    if (event?.toolName === "ask") ask(ctx);
  });
  on("tool_execution_end", (event, ctx) => {
    if (event?.toolName === "ask") answered(ctx);
  });
}
