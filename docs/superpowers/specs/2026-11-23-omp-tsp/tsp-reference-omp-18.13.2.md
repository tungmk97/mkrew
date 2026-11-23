# Tern Surface Protocol (TSP) as implemented by omp 18.13.2 - protocol reference

Source: `C:/Users/NguyenTung/.bun/bin/omp.exe` (bun-compiled, JS embedded). Everything below was read from the
embedded minified-but-readable JS (and `omp://` docs where noted). Nothing was executed.

## 0. Citation key and files

Every claim carries `[tag@N]`: module tag + **absolute byte offset N in omp.exe** of the cited function/constant/snippet.
Offsets of module headers (`// packages/...ts`) are in `module-index.tsv` (offset, size, path). Resolved snippet offsets are in
`_offsets.json`. Local code listings use the same names, e.g. `[encode@178324375]` → find `function T(A, e, t)` in
`extra/tui__src__native__encode.ts.178323243.js`.

| tag | module | header offset | extracted file (under `omp-tsp/`) |
|---|---|---|---|
| tsp | packages/wire/src/tsp.ts | 175961113 | `packages__wire__src__tsp.ts.175961114.js` |
| encode | packages/tui/src/native/encode.ts | 178323242 | `packages__tui__src__native__encode.ts.178323243.js` |
| backend | packages/tui/src/native/backend.ts | 178548901 | `packages__tui__src__native__backend.ts.178548902.js` |
| apply | packages/tui/src/native/apply.ts | 178515734 | `..native__apply.ts.178515735.js` |
| reconcile | packages/tui/src/native/reconcile.ts | 178527486 | `..native__reconcile.ts.178527487.js` |
| describe/state/node/memo/settle/spans/icons/tone/blobs/overlay/picker | packages/tui/src/native/*.ts | 178320086…179252446 | `packages__tui__src__native__*.js` |
| term | packages/tui/src/terminal.ts (ProcessTerminal) | 178429919 | `extra/tui__src__terminal.ts.178429920.js` |
| tui | packages/tui/src/tui.ts | 178566894 | (read in-memory; not saved separately) |
| stdin | packages/tui/src/stdin-buffer.ts | 178403628 | `extra/tui__src__stdin-buffer.ts.*.js` |
| winin | packages/tui/src/windows-input-mode.ts | 178422293 | (in-memory) |
| hub | packages/tui/src/overlays/agent-hub.ts (live subagent Agent Hub, class `yle`) | 189094716 | `packages__tui__src__overlays__agent-hub.ts.189094717.js` |
| hubs | packages/tui/src/overlays/agents-hub.ts (`/agents` *definitions* hub - NOT the roster) | 189152709 | `..agents-hub.ts.189152710.js` |
| hubren | packages/tui/src/overlays/agent-hub-renderer.ts | 178956318 | `..agent-hub-renderer.ts.178956319.js` |
| hubproj | packages/tui/src/overlays/agent-hub-projection.ts | 185542336 | `..agent-hub-projection.ts.185542337.js` |
| jobs | packages/tui/src/overlays/jobs-panel.ts (class `aEe`) | 188315919 | `..jobs-panel.ts.188315920.js` |
| im | packages/coding-agent/src/modes/interactive-mode.ts | 191209184 | `extra/coding-agent__src__modes__interactive-mode.ts.191209185.js` |
| sel | packages/coding-agent/src/modes/controllers/selector-controller.ts | 191142248 | (in-memory) |
| composer | packages/tui/src/prompt/composer.ts | 184950194 | `extra/tui__src__prompt__composer.ts.*.js` |
| ce | packages/tui/src/prompt/custom-editor.ts | 180165557 | `extra/tui__src__prompt__custom-editor.ts.180165558.js` |
| ed | packages/tui/src/components/editor.ts | 179374954 | `extra/tui__src__components__editor.ts.*.js` |
| sl | packages/tui/src/status-line/component.ts | 184863346 | `extra/tui__src__status-line__component.ts.184863347.js` |
| job | packages/tui/src/tools/job-widget.ts | 180096745 | (in-memory) |

Also saved alongside: `extra/` (182 extracted modules: all `tui/src/native`, `components`, `chrome`, `prompt`, `status-line`,
`overlays/{agent-hub*,hub-frame,jobs-panel,model-hub,…}`, `terminal.ts`, `stdin-buffer.ts`, `interactive-mode.ts`, …),
`module-index.tsv`, `_offsets.json`, `kind-props.json` (per-kind prop keys + call sites with offsets), `roles.txt`
(485 `omp.*` role strings), `icon-names.txt`.

Docs cross-checked with `omp://tui-core-renderer.md` (§ "Native rendering (Tern Surface Protocol)" and § "Explicit composer
submission"), `omp://tui-runtime-internals.md` (§ "Native surfaces"), `omp://environment-variables.md`, `omp://agent-hub.md`.

---

## 1. Transport

### 1.1 Constants
- `J7 = 1` protocol version [tsp@175961142]; `k_t = "tsp"` tag [tsp@175961154]; `Q7 = 65536` default max APC body bytes [tsp@175961171];
  `iUe = 2` default frame credits [tsp@175961187].
- `oUe` = the 43 node kinds (§4) [tsp@175961200]; `sUe` = text-bearing kinds [tsp@175961680].

### 1.2 Byte framing (both directions)
```
ESC _ t s p ; <verb> { ; <key>=<value> } ; <body> ESC \
0x1B 0x5F                                        0x1B 0x5C   (APC … ST)
```
- `dA = "\x1B_"` [encode@178323280]; `XI = dA + "tsp" + ";"` [encode@178323316]; terminator `j = "\x1B\\"` (ST, never BEL).
- Writer `T(verb, paramStr, body) = XI + verb + paramStr + ";" + body + ST` [encode@178324375]; `xA` renders params as `;k=v` per key
  [encode@178324434]. So no params → `ESC_tsp;q;{json}ESC\`; with params → `ESC_tsp;b;id=<hex>;mime=image/png;<base64>ESC\`.
- Parser `Pye` [encode@178325222]: must start with `XI` and end with ST; verb = text up to first `;` (no `;` ⇒ verb only, empty body);
  then repeatedly consumes `k=v;` segments while they match `^[A-Za-z0-9_-]+=[\x21-\x3a\x3c-\x7e]*$` (value = printable ASCII except `;`);
  everything after is the body.
- Encoding: JSON bodies are `JSON.stringify(obj)` - compact, UTF-8, no spaces (`t$e` = `e$e(verb, JSON.stringify(x), params, limit)`
  [encode@178324551]). `JSON.stringify` escapes all control chars so a body never contains ESC/ST (derived from JSON spec).
  The blob body (verb `b`) is raw base64 text.
- Verbs omp → terminal: `q` query, `o` open surface, `t` theme, `f` frame, `x` close surface, `b` blob.
  Verbs terminal → omp: **only `r` (reply) and `e` (event)**; any other verb is dropped: `rIt` returns null unless verb is `r`/`e`, and the body
  must be a JSON object [encode@178327590].

### 1.3 Chunking (65536 limit)
`e$e(verb, body, params, limit=Q7)` [encode@178324551]:
1. `body.length*3 <= limit` → one message.
2. else `Buffer.byteLength(body) <= limit` → one message.
3. else split into chunks of ≤ `limit` **body bytes** (prefix/params excluded). Chunk id `c` = module-global counter `NA++` in base36 starting `1`
   [encode@178323280 region, `var NA = 1`]. Chunk i is `T(verb, (i==0 ? params : "") + ";c=<id>" + (i<last ? ";m=1" : ""), chunk)`.
   Chunk cut points: `UA` backs off to a UTF-8 char boundary and to a position where the next text does not look like `key=value;`
   (`GA`), so a chunk body can't be mis-parsed as params [encode@178323707, @178323353]. Pure-ASCII bodies without `;` are cut by plain slicing (`RA`).
- Reassembly `n$e.feed` [encode@178327215]: if param `c` present, accumulate by id; `m=1` ⇒ hold and return nothing; otherwise concatenate and parse.
- Effective limit: hello reply `apc` if `> 0`, else `Q7` (`#Y` in backend: `this.#s = n.apc > 0 ? n.apc : Q7`) [backend@178551801 region].
- **Terminal→omp chunking works for events** (they go through `handleInput → n$e.feed`) [backend@178556437] **but not for the hello reply**: the
  hello is parsed with `iIt`, which ignores `m=1`, and a chunked hello that falls through to `feed` is not handled either [term@178452003, encode@178327590].
  [INFERENCE] keep the hello reply in one APC.

### 1.4 Direction omp → terminal
omp writes the APC string with `terminal.write()` → stdout through its output pump [term@178460639]. `write()` prepends `ESC[?2004h` when it
believes bracketed paste is gated (`#N && #QA`) - harmless to a TSP parser. All TSP bytes are therefore in the **PTY output stream, interleaved with
ordinary escape output**; the terminal emulator must strip them from the cell grid.

### 1.5 Direction terminal → omp (replies/events)
The terminal writes the same APC framing into the **PTY input** (omp's stdin). Path:
`process.stdin` → `StdinBuffer` (recognises `ESC _ tsp;` as an APC start via regex `vA` `\x1b_(?:G|t(?:s(?:p(?:;|$)|$)|$)|25a1…)` [stdin@178403991],
holds partial sequences) → `ProcessTerminal.#K`: while `s.startsWith(XI) || s.startsWith(L)` it accumulates until `pi()` says complete
[term@178446248] → `#_A`: hello reply handled in place, everything else forwarded → `TUI.#dt`: `if (n.startsWith(XI)) { if (native) backend.handleInput(n); return; }`
[tui@178602076] → `NativeBackend.handleInput` → `Pye`/`feed` → `e` events dispatched, `r:"blobs"` replies handled [backend@178556437].
- Alternate envelope accepted from the terminal: `ESC ] 877 ; tsp ; … (BEL | ESC \)` - `L = "\x1B]877;" + "tsp;"` is rewritten into the APC form by `pi()`
  [term@178430051, @178430508]. [INFERENCE] intended for hosts/multiplexers that forward OSC but swallow APC.
- Events are only processed while native mode is active; before the hello reply is accepted, only `r:hello` is consumed.
- Windows/ConPTY: omp enables win32-input-mode (`ESC[?9001h`, decoder `Jlt`/`Qlt`) only when the kitty keyboard reply isn't seen within 150 ms and ConPTY mode is on
  [term@178453575, @178454183, winin@178425611]. `Qlt.process` passes any byte run that isn't `ESC [ … _` through unchanged and decodes `ESC[Vk;Sc;Uc;Kd;Cs;Rc_` key records
  to chars [winin@178427570], so a raw APC, **or** the same bytes encoded as win32 key events, both reach the TSP parser.
  [INFERENCE / RISK] whether Windows' conhost forwards a raw APC written to ConPTY input and passes an APC emitted by the child through output is unverified here; spike it first.

### 1.6 Env / enablement (as read)
`PI_TUI_NATIVE=0` never probes; `=1` forces (still needs tmux `@tern-tsp`); unset probes every direct terminal; multiplexers (`ownsScreenGrid`), `DEVENV_RELOAD_FILE`, bun test runtime skip
[term@178450569]. `TERM_PROGRAM=tern` makes `tspExpected` true (optimistic surface, §2.4) [term@178438020]. `PI_TUI_TSP_RECORD=<file>` records every in/out message as JSONL
`{t,dir,verb,params,body}` [backend@178560865 `#X`]; `PI_TUI_NATIVE_STATS=1` logs ops/rows-fallback counts; `TERN_BLOB_DIR` enables file-based blob cache (§6.4).

---

## 2. Handshake

### 2.1 Probe bytes
`#ZA` [term@178451094] writes (non-tmux) `Ylt()` + `ESC[c` (DA1) and pushes `{kind:"tsp"}` onto the DA1-sentinel FIFO. `Ylt()` [encode@178325029] =
```
ESC _ tsp;q;{"q":"hello","v":[1],"app":"omp","features":["edit","undo","send"]} ESC \   ESC [ c
```
(`ver` is passed `undefined` ⇒ omitted by JSON.stringify.) `features` are what **omp** supports on the terminal's behalf: `edit` (`e:edit`), `undo`, `send` (explicit composer submission).

### 2.2 Reply
```
ESC _ tsp;r;{"r":"hello","v":1,"term":"<name>","kinds":["col","row",…],"features":["blobs","scroll",…],"apc":65536,"credits":2,"cols":120,"dark":true,"reduceMotion":false,"hour12":true} ESC \
```
- Validator `SA` [encode@178325885]: `v` number, `term` string, `kinds` string[] are required; everything else optional (blobs reply: `{"r":"blobs","have":[ids]}`).
- Use of fields (class `Le` [backend@178549714], `#Y`): `kinds` → `cx.supports(kind)`; `features` → `cx.feature(name)`; `cols` (default = PTY columns); `dark`; `reduceMotion`; `hour12`; `apc` (max body bytes; default 65536); `credits` (max unacked frames; default 2).
- Accepted only if `v === 1` (`J7`); otherwise `i("TSP: unsupported protocol version")` and omp stays on rows [term@178452003].
- Features omp checks: `blobs` [backend@178558435], `scroll` [reconcile `r.feature("scroll")`, @178529465 region]. Kinds omp's components test via `supports()`: `picker`, `tree`, `meter`, `tool`, `media`, `effort`, `checklist`
  (agent-hub `describe`, model-hub, form, loader, tool-execution, custom-editor `describeLayout`, interactive-mode `describeStatusHud`).
- omp's own assumed hello for Tern (`nIt`) [backend@178550340]: `{r:"hello", v:1, term:"tern", kinds: all 43 minus "media", features:["blobs"], apc:65536, credits:2, cols, dark, reduceMotion:false}` - a good minimal reference reply.

### 2.3 DA1 sentinel and fallback to rows
DA1 replies are matched FIFO against sentinel owners. If a DA1 reply (`ESC[?…c`) pops the `tsp` owner before a hello reply was seen →
`#yA(null, "DA1 was answered before a hello reply: the terminal, or a layer in between, dropped the APC query")` ⇒ rows renderer [term@178444576].
**The terminal must emit the hello reply before answering the DA1 that follows the query** and must still answer DA1 when it doesn't speak TSP.
tmux variant: no DA1; waits `wA = 1000` ms, sets pane option `allow-set-title off`, requires tmux option `@tern-tsp` [term@178430086, @178451094].
Rows fallback after a failed handshake: `TUI.#Et` logs "terminal did not confirm the surface; falling back to rows", `backend.stop(false)` → `x {keep:false}` [tui@178584764].

### 2.4 TUI side of the handshake
- `TERM_PROGRAM=tern` (`tspExpected`): omp **opens a surface optimistically** with the synthetic hello `nIt(terminal)` immediately (`o`, `t`, first `f`), arms a 1000 ms check (`kt`); no confirmation ⇒ fallback to rows; confirmation (`#hn` → `backend.confirm(realHello)`) swaps in the real context
  (re-render if dark/motion/kinds/features changed) [tui@178582694, @178573757, @178583565].
- Otherwise the first row paint is deferred up to `rn = 300` ms waiting for the hello [tui@178573715]; a supported hello then starts native mode (`#Nt` → `new NativeBackend(...).start()`) [tui@178584075].
- While native is live omp forces the nerd symbol preset (`TN("nerd")` at start, `TN(undefined)` at stop) [backend `#H`, theme@176887061] so icons are PUA glyphs (§6.2).

### 2.5 Surfaces, ids, roots
- Surface id = `s:<n>`, `n` from 1 per backend instance (`#G` → `` `s:${this.#m++}` ``) [backend@178551801 region]. First surface is `mode:"inline"`, `role:"omp.session"` (`Oe`) [backend@178549190].
- `o` (open) body: `{"id":"s:1","mode":"inline","title":"omp","role":"omp.session"}`, plus `"adopt":true` on resume after a TUI stop/start [backend@178553833, `#se`].
  A second surface `mode:"screen"` is opened for a fullscreen overlay that is neither a `nativeSheet` nor has `nativeOverlay` (role from `describeScreen()` or `omp.session`) and closed with `x` when it goes away [backend@178554860 `render`].
- `x` (close): `{"id":"s:1","keep":true|false}`; `keep` = `stop(n=true)` default [backend@178553512]. [INFERENCE] `keep:true` = leave the rendered content in the terminal.
- `t` (theme) immediately after `o`: `{"sf":"s:1","dark":{<token>:"#rrggbb",…},"light":{…},"name":{"dark":"…","light":"…"}}` - each omp theme token as hex for the dark and/or light variant; re-sent when the resolved palette changes
  (`#U`, `#V`; `EN()` [theme@176893958]; only sent when dark or light non-empty) [backend@178553087]. Token set: `accent border borderAccent borderMuted success error warning muted dim text thinkingText selectedBg userMessageBg userMessageText customMessageBg customMessageText customMessageLabel toolPendingBg toolSuccessBg toolErrorBg toolTitle toolOutput md* toolDiff* syntax* thinking{Off…Xhigh,Max} bashMode pythonMode statusLine*` + export `pageBg cardBg infoBg` (schema [193607070]).
- Document roots: the first frame of every surface adds three empty `col` roots under the surface id: `["add","main","s:1",null,{"id":"main","k":"col"}]`, same for `"dock"` and `"layer"` [reconcile `Me = ["main","dock","layer"]` @178527557, `reconcile()` @178529465].
  - `main` = flowing transcript (`Composer.describeSurface().main`), `dock` = live chrome pinned at the bottom (editor, status, pills; §8), `layer` = overlays/sheets (§7) [composer@184956759, tui@178585580].
  - `detachLive()` (used on resume) removes `dock`/`layer` contents and re-adds them next frame.
- Terminal `gone` for the inline surface id ⇒ omp drops everything, creates a new surface `s:<n+1>`, re-opens and repaints [backend@178561569 `case "gone"`].

---

## 3. Frames, ops, pacing, errors

### 3.1 Frame
`f` body: `{"sf":"s:1","s":<seq>,"ops":[ … ]}`; `s` starts at 1 and increments per sent frame; empty op list ⇒ nothing is sent [backend@178558040 `#ae`, `render` @178554860].
Frames are produced by re-`describe()`-ing the whole live tree each render and diffing against the previous one in `Reconciler.reconcile` (not by incremental pushes) [reconcile@178529465].
Blob messages (`b`) needed by the frame are written *before* it (`#le`) [backend@178558435].

### 3.2 Ops (all are JSON arrays; `null` = none) - executor `Glt.#m` [apply@178517337]
| op | form | semantics |
|---|---|---|
| add | `["add", id, parent, before\|null, node]` | `node = {id,k,p?,c?}` (children nested, ids unique); `node.id` must equal `id`; `before` must be a child of `parent`; unknown kind / duplicate id are errors |
| set | `["set", id, {prop:value,…}]` | merge; `null` deletes the prop; `undefined` is not sent |
| text | `["text", id, "append"\|"replace", string]` | only on text-bearing kinds (`sUe`), edits `p.text` |
| splice | `["splice", id, from, delCount, insertText]` | UTF-16 index range inside `p.text`; emitted only for `editor`/`input`/`md` (md only if common prefix > 0) [reconcile `#I`@178539234] |
| move | `["move", id, parent, before\|null]` | reparent/reorder; cannot move root or into own subtree |
| del | `["del", id]` | removes node and subtree; clears focus if inside |
| settle | `["settle", id]` | hint: subtree is final (finished transcript block) - emitted once per settled component (`kye()`/inherits from settled parent) [reconcile@178530921, settle@178527371] |
| focus | `["focus", id\|null]` | omp's focused component → editable node id (`focusTarget`: first `editor`/`input` under the focused component) [backend render, reconcile@178532364] |
| reveal | `["reveal", id, at]` | scroll node into view; `at` = `"start"\|"end"\|"nearest"`; from node field `reveal: "x"` (on add) or `reveal:{n,at}` when `n` changes; selected list items get `"nearest"` |
| scroll | `["scroll", id, by]` | only if hello `features` has `scroll`; from node field `scroll:{n,by}`; `by:"start"\|"end"` ⇒ 1 op, else up to 32 repeats (`_n=32`) [reconcile@178537568] |
| suspend / resume | `["suspend"]`, `["resume"]` | implemented by the reference doc, but **no emitter seen** in reconcile/backend [INFERENCE: reserved] |

Op order inside a frame: `add`/`move` pass (keyed with an LIS to minimise moves), then removals (`del`), parked-node `del`s, `settle`s, `reveal`s, `scroll`s (reconcile@178529465); backend appends `focus` last when the target changed.
`node`/`reveal`/`scroll`/`key` are *node-level* fields (not in `p`).

### 3.3 Node ids
`id = <base>.<keypath>`; `base` ∈ `main|dock|layer` for nodes owned by a surface root, or a **component id** (base-36 counter `ZBe`, never equal to a root name) for a native component's subtree, e.g. `k.line/input`.
`keypath` = sibling keys joined by `/`, key = node `key` or its index; `% / ^` in keys are escaped `%25 %2F %5E`; duplicate sibling keys get `~2`, `~3`…; overlays/pickers hoisted out of a tree get a `^` keypath prefix (portal) [reconcile@178527896, `#S`]. Components' own roots have id = component id (no dot).

### 3.4 Acknowledgements and pacing
- Terminal → `{"ev":"ack","sf":"s:1","s":N}` (cumulative; `sf` optional ⇒ inline surface). omp keeps `unacked` timestamps; a new frame is sent only while `unacked.length < credits` (hello `credits`, default 2) [backend@178557307 `#M`].
- If at credit limit, the render is marked dirty and re-requested on the next ack, or after 5000 ms (`Rt`) stall: omp logs "terminal stopped acknowledging frames; resuming without credits", clears the unacked list and sends anyway [backend@178549175].
- `ack` handler clamps `s` to the last sent seq, drops that many unacked entries, cancels the stall timer and re-renders if dirty [backend@178561569].
- A terminal that never acks degrades to one frame / 5 s. **Ack every frame.**

### 3.5 Error reporting
- omp-side reference document (`PI_TUI_TSP_RECORD`/debug socket `doc` op) applies every frame it sends via `Glt.applyFrame`, collecting `{s, op:<opIndex>, msg}` (e.g. `unknown id X`, `duplicate id`, `X (k) has no primary text`, `splice range a+b outside text of length n`) - logged only (`"TSP: reference document rejected ops"`) [apply@178516558, backend `#ae`]. Those errors are **not sent** to the terminal.
- Terminal → omp: `{"ev":"error","msg":"…"}` is logged (`"TSP: terminal reported an error"`) and otherwise ignored [backend@178561569].
- `{"ev":"gone","ids":[…],"sf"?:…}`: terminal dropped these node ids; omp permanently suppresses ops that mention them (`forget` → filter `#b`: `add/move` check id/parent/before, others check target id) [reconcile@178532517].

### 3.6 Settle / cadence
`settle` lets the terminal freeze finished blocks. omp renders on `requestRender()` (coalesced); native frames are paced by acks, not by a fixed rate; `scheduleRender` is used only for the stall timer, hello timeouts and blob timeouts.

---

## 4. Node kinds (43) and props

Props below = keys seen in omp's `s("<kind>", {…})` call sites across `tui/` and `coding-agent/` (full counts + call-site offsets in `kind-props.json`) plus component `describe()` code. `k`=kind, `p`=props, `c`=children, `key`=stable key. Builder helpers [describe@178320087]: `s(k,p,c,key)`, `t(text,style,extra)` → span `{t,s?,…extra}`, `c(textOrSpans,p)` = `text` node, `k(children,p)` = `col`, `J` = `row`, `Un(p,c)` = `card`, `Ze` = `md`, `In` = `code`, `Qr` = `ansi`, `$ye` = `diff`, `Hi(key,p)` = `item`, `vb` = `list`, `Zi(ms,stopped)` = `elapsed`, `sa("ctrl+x")` = `kbd{keys:[…]}`, `Wne` = strip components from a subtree (for `hovercard`).

**Text-bearing kinds (`sUe`)**: `text md code ansi math editor input shimmer` - `p.text` is changed only with `text`/`splice` ops (never `set`); reconcile diffs: new text startsWith old ⇒ `text append`; for `editor`/`input` (and `md` with common prefix) ⇒ `splice` of the middle; else `text replace`; `text` removed ⇒ `set text:null` [reconcile@178538425, @178539234]. Text containing PUA icon glyphs is converted to `spans` before this (icons, §6.2) - a `text`/`shimmer` node may therefore carry `spans` instead of `text`.

**Common props** (any kind): `role` (semantic style hook `omp.*`; 485 strings in `roles.txt`, e.g. `omp.btn omp.actions omp.overlay.agentHub omp.jobs.master omp.composer.bar omp.hud.seg`), `tone` (`neutral accent info success warning pending error muted user`), `title` (tooltip), `aria`, `hidden`, `grow/shrink/basis` (flex), `min/max` (`{w:"48ch"|0,h:"16lines"}` or `{lines:N}`), `gap` (`none xs sm md lg`), `align` (`start center end baseline stretch`), `justify` (`end between`), `wrap` (`true|"word"|"none"|"char"`), `truncate` (`end middle start`), `lines`, `href`, `actions` (`{click:"act", dblclick:"act", menu:[…]}`; list default `{click:"select",dblclick:"activate"}`), `hovercard` (inline subtree, `row` in jobs pill [im@191214775]).

| # | kind | props omp sends | children | notes / evidence |
|---|---|---|---|---|
| 1 | col | role gap grow min max basis shrink align tone hidden title actions mark | any | vertical flex; `hidden:true` empty col = "render nothing" (`CR`) |
| 2 | row | gap align justify wrap role title actions grow shrink min tone href aria hovercard | any | horizontal flex; used for buttons (`omp.btn`, [overlay@178522661]) |
| 3 | card | head role tone collapsible collapsed preview status inset | any | `head` = spans/string or child with key `head`; collapsible ⇒ `toggle` events; `preview:{lines}` while collapsed |
| 4 | section | head role collapsible collapsed tone actions | any | titled group; collapsible via `toggle` |
| 5 | rule | label role | – | separator; `label` spans/string |
| 6 | spacer | grow \| size | – | flex gap |
| 7 | text | spans \| text, wrap truncate lines role tone actions title grow hidden shrink | – | text-bearing; spans `{t,s,href,fx}` (§6.1) |
| 8 | md | text role stream | – | text-bearing; `stream:true` for in-flight markdown [markdown.ts describe] |
| 9 | code | text lang wrap preview:{lines} role tone | – | text-bearing; e.g. `lang:"bash"` |
| 10 | diff | text \| hunks, path, mode `unified\|split`, role | – | `text` = unified diff (git-tui) or `hunks` (chrome/diff.ts) |
| 11 | ansi | text follow preview:{lines} max:{h} role grow | – | text-bearing; **terminal must interpret SGR** inside; `follow` = tail while growing |
| 12 | math | text display | – | text-bearing LaTeX [latex-block.ts@178753682] |
| 13 | image | blob(id) alt w h max:{w,h} role builtin href actions:{menu:["open"]} | – | bytes via `b` (§6.4) [blobs@178477129] |
| 14 | media | path max:{h} link grow role | – | file path on disk; **Tern's own hello omits `media`** (tool results then use `image`) [blobs@178478009, backend `Jn`] |
| 15 | kv | items:[{k,v}] layout(`grid`) role | – | `k`,`v` string or spans |
| 16 | table | cols:[{id,head,align:`start\|end`,truncate,priority,format}] rows:[{id,cells:{<colId>:text\|spans}}] role | – | components/table.ts: ids `c0…`, `r0…` |
| 17 | tree | nodes:[{id,label(spans),children?,open?}] | – | components/tree-view.ts; omitted if hello lacks `tree` (hub then uses `list`) |
| 18 | badge | text tone title role | – | pill |
| 19 | kbd | keys:[…] role title tone actions | – | key caps; `esc` for escape (`Qkt`) |
| 20 | icon | name tone title aria role actions | – | named icon (`copy x eye eye-off model chev jobs history link open pause rewind check lightbulb wand gauge loop back corner-down-right`; also item icons like `agents users job bash agent folder plus list message wrench x-circle send activity`; full list `icon-names.txt`) |
| 21 | spinner | label(spans) style tone role | – | style `braille dots orbit starburst…` |
| 22 | shimmer | text\|spans mode palette:{low,mid,high} role | – | text-bearing animated text |
| 23 | elapsed | age(ms) stopped(ms)? format:"short" role title | – | terminal ticks locally until `stopped` is set |
| 24 | progress | value(0..1\|null) label tone max title grow | – | |
| 25 | rate | value unit("tok/s") role title | – | |
| 26 | list | selected(item id) empty(spans/string) virtual filter max:{lines} role grow aria tone actions | `item`s | omp rewrites `selected` from item *key* to node id [reconcile `#w`@178538425 region] and emits `reveal nearest` for it |
| 27 | item | label(spans) detail value icon tone disabled role hint title aria | – | `key` = id used in `select/activate` |
| 28 | tabs | items:[{id,label}] active role actions title | – | events `select/activate` item=tab id |
| 29 | editor | text cursor sendable anchor decor:[{from,to,s,fx}] ghost placeholder mode lang maxLines readonly role | – | text-bearing; offsets are UTF-16 of whole text; see §5/§8 [ed@179399521] |
| 30 | input | text cursor sendable prompt placeholder ghost readonly role grow | – | text-bearing single line [components/input.ts] |
| 31 | status | transparent role grow | `seg`s | bar that drops low-priority segs on overflow |
| 32 | seg | side(`left\|right`) priority spans icon tone title min actions role | – | status/composer facts [sl@184929886] |
| 33 | overlay | anchor size modal role head | 1 child | §7 [backend@178565067] |
| 34 | toast | text ttl role | – | status notice [chrome/status-notice.ts@188723222] |
| 35 | rows | cols lines:[ANSI string] | – | **omp-generated fallback only** (§10) [reconcile@178544240] |
| 36 | picker | title subtitle icon noun size layout(`rows\|tree`) preview(`side\|none`) query cursor placeholder tabs tab scopes scope columns items selected total state message empty actions strip focus current confirm order | detail preview nodes | §7 [picker@179252484] |
| 37 | prefs | title pages page lead sections focus editing query cursor | editor/picker | settings sheets (settings-selector, advisor-config) |
| 38 | tool | role key name title target targetKind lang command href meta badges note exit status age took intent frame(`card\|inline`) collapsible collapsed preview tools tone | body nodes | tool-call card; omp uses a `card` instead when hello lacks `tool` [chat/tool-execution@181527002] |
| 39 | checklist | mode(`full\|reminder\|hud`) phases note role | – | todo list; fallback when `checklist` unsupported |
| 40 | agent | name task status model tool:{name,intent,age}\|null retry:{attempt,…} stats role | – | task-tool subagent card (tools/task.ts), cleanse board |
| 41 | chart | kind:`spark` size series:[{value}] min max token summary cells cols rows tips role | – | job widgets / hud spark |
| 42 | meter | value(0..1\|null) style(`bar\|ring\|blocks\|segmented`) size label total thresholds parts marks steps tone title actions aria role | – | context meter [sl `#Dt`@184929886], job ring; hub falls back to `progress` if hello lacks `meter` |
| 43 | effort | level role title actions | – | thinking-effort glyph; else `meter{style:"blocks",steps:4}` [ce@180185459] |

Counts from the call-site scan (sites): col 86, row 114, text 79, icon 42, item 41, spinner 39, section 38, list 35, kv 28, badge 28, … `rows` 0 (generated), `prefs` 2, `agent` 2, `toast` 1, `math` 1 (`kind-props.json`).

---

## 5. Events terminal → omp (`e` verb)

Envelope: `ESC _ tsp;e;{"ev":"<kind>","sf":"s:1",…} ESC \`. Validation `HA`/`mA` [encode@178326719, @178326115]: `ev` string; per-kind required typed fields (below); unknown `ev` or wrong types ⇒ silently dropped. `send` has its own check.

| ev | required fields | extra (unvalidated) | omp effect |
|---|---|---|---|
| ack | s:number | sf | credits (§3.4) |
| resize | cols:number | sf | sets `cx.cols`, re-render (used for `rows` width). Marks "terminal drives width" so PTY SIGWINCH no longer updates cols [backend@178561569; tui@`noteTerminalColumns`] |
| theme | dark:boolean | | `cx.dark`, re-queries OSC 11 (`appearanceChanged → refreshAppearance`) [tui@178585580] |
| motion | reduce:boolean | | `cx.reduceMotion`, invalidate + render |
| visible | visible:boolean | | ignored |
| toggle | id:string, collapsed:boolean | | `handleNativeEvent({type:"toggle",key,collapsed})` on owner component |
| select | id, item:string | | `{type:"select",key,item}` - item may be a node id or key; omp maps via `itemKey` |
| activate | id, item:string | | `{type:"activate",key,item}` (dblclick/enter) |
| action | id, act:string | value, mods:string[] | `{type:"action",key,act,value,mods:mods??[]}` - button clicks, tab/scope picks, `close` |
| change | id, item:string | value | `{type:"change",key,item,value}` (prefs sheets) |
| edit | id, from,to,cursor,len:number, text:string | | host-side edit of `editor`/`input`: `wye()` [node@179232521] validates `len === current text length`, snaps to surrogate boundaries, applies, sets cursor [ed@179433146 `applyHostEdit`] |
| undo | id | | `{type:"undo",key}` |
| focus | id | | `owners(id)` (component → containers) → `TUI.focusFromPointer`; picks the outermost owner (`findLast`) with `handleInput` whose `focusTarget === id` |
| send | sf:string(nonempty), id:string(nonempty), text:string | | atomic prompt submit (below) |
| error | msg:string | | logged |
| gone | ids:string[] | sf | §3.5 |

Dispatch `#ue` [backend@178563675]: `target(id)` splits `id` at the first `.` → component id → component, `keypath` = rest (split on `/`, unescaped, leading `^` stripped) [reconcile@178531720]; events on nodes whose owner is a surface root (`main.…`, `dock.…`, `layer.…`) have no component and are ignored. Item args for `select/activate`: if item starts with `<id>/` or `<id>.` or has no `.`, `itemKey()` converts a node id to the node's `key` [reconcile@178532174]. After the handler omp calls `requestRender()`.
**`send`**: honoured only if native mode is live **and** `reconciler.focusTarget(component) === id` (id must be the `editor` node, e.g. `k.line/input`) [backend `#ue` first check]; component receives `{type:"send",key,text}`; `CustomEditor.handleNativeEvent` ignores empty/whitespace text or `!nativeSendable`, queues behind in-flight paste work, else clears draft and `submit()`s [ce@180188826; docs "Explicit composer submission"]. Only submit when the `editor` node has `sendable:true` (`nativeSendable` = has `onSubmit` && !`disableSubmit` && not dictating) [ce@180182088].

There is **no** wire event for raw key, click, wheel or scroll. Pointer gestures are expressed through each node's `actions` (`click`, `dblclick`, `menu`) and surface as `action`/`select`/`activate`/`toggle`/`focus`.

### 5.1 Keyboard while a surface is live
Raw keys still go through the PTY: `ProcessTerminal` keeps raw mode, kitty keyboard / modifyOtherKeys negotiation (`ESC[?u`, `ESC[>1u`…), and `TUI.#dt` runs the same input pipeline (ctrl+c/escape timers, global handlers, focused component `handleInput`) - only strings that start with the TSP prefix are diverted to the backend [tui@178602076, term@178454183]. So `Esc`/`j`/`k`/`Enter`/`x`/`r`… in Agent Hub and Jobs work exactly as in the TUI when the terminal forwards keys to the PTY. `edit`/`undo`/`send` are the *optional* path for a terminal that edits text locally (features advertised in the query); `focus` events let the terminal move omp's focus on pointer click. Keys for raw-mode setup that omp itself sends: `ESC[?2004h` (bracketed paste), `ESC[?1l ESC>`, `ESC[?2031h`, DECRQM probes `ESC[?<n>$p` + DA1 for 2026/2048/2031/2004/1010/1011, OSC 11 query, kitty `ESC[?u`, optional OSC 99 and Glyph Protocol (APC `25a1`) probes - each followed by a DA1 sentinel [term@178454183, @178456000 region, `#CA`, `#OA`, `#XA`].

---

## 6. Spans, tone, icons, blobs

### 6.1 Styled spans
Span = `{"t":"text","s":"<space-separated style tokens>","href":"url","fx":"pulse|shimmer"}` (all but `t` optional). `text.spans`, `item.label/detail/value`, `card/section.head`, `kv.items[].k/v`, `tabs.items[].label`, `table` heads/cells, `tree` labels, `seg.spans`, `rule.label`, `spinner.label`, `progress.label`, `editor/input.prompt`, `list.empty` accept string **or** span array (`hc()` collapses a single unstyled span to a string) [icons@178523614 `On`, spans@178470243].
Style tokens: (a) any theme fg token name from the `t` message (`accent error success warning muted dim text info statusLineCost customMessageLabel thinking* …`); (b) modifiers `strong dim em del mark`; (c) semantic: `mono path num code link icon ins del hide typo bashMode pythonMode`; (d) `fx` motion tokens.
ANSI→spans (`wr`) [spans@178470243]: SGR 1→`strong`, 2→`dim`, 3→`em`, 9→`del`, 7→`mark`; fg colour reverse-mapped to a theme token via the live theme's ANSI strings, falling back to base-16 `31/91 error, 32/92 success, 33/93 warning, 34/94 info, 35/95 accent, 36/96 info, 90 dim` (`Zt` [spans@178466276]); `text` colour dropped; bg `selectedBg` → `mark`; other colours/bgs dropped; OSC 8 → `href`; every other CSI/OSC/APC stripped; adjacent equal-style spans merged.

### 6.2 Icons
Two mechanisms: (1) node `icon{name}` and `item.icon`/`seg.icon` named icons (`icon-names.txt`); (2) **glyph spans**: because omp forces the *nerd* symbol preset in native mode, theme symbols are Private-Use-Area codepoints (`U+E000–F8FF`, `U+F0000–FFFFD`, `U+100000–10FFFD`). `tIt` splits every PUA run out of `text/shimmer/seg/card.head/section.head/overlay.head/rule.label/spinner.label/progress.label/list.empty/item.{label,detail,value}/editor|input.prompt/kv/tabs/table/tree` strings into its own span and appends the token `icon` to its style (`s:"muted icon"`), trimming one adjacent space [icons@178523910 `An`, @178523614 `On`, regex `Ee` @178523466]. The terminal supplies the nerd-font glyphs itself.

### 6.3 Tone / colours
`tone` ∈ `neutral accent info success warning pending error muted user`. Mapping from omp bg tokens (`Wge` [tone@178632027]): `userMessageBg→{tone:"user",role:"omp.user"}`, `customMessageBg→{info,"omp.custom"}`, `toolPendingBg→{pending,"omp.tool"}`, `toolSuccessBg→{success}`, `toolErrorBg→{error}`, `selectedBg→{accent,selected:true}`, `statusLineBg→{neutral,"omp.status"}`. Tone→fg token (`Iu` [tone@178632631]): neutral→text, accent/info→accent, success→success, warning/pending→warning, error→error, muted→muted, user→customMessageLabel. Colours are never sent as RGB in nodes - only token names; RGB comes from `t`.

### 6.4 Blobs/images
- Blob id = SHA-256 hex of the bytes; `mime` kept [blobs@178476688]. `image` node: `{k:"image",p:{blob:"<id>",alt,w,h,max:{w:"12ch",h:"4lines"}…}}`; `w/h` = pixel size when known [blobs@178477129].
- Before a frame containing new blob ids omp resolves them (`#le`) [backend@178558435]:
  1. hello lacks feature `blobs` ⇒ every blob is sent inline right away.
  2. `TERN_BLOB_DIR` set ⇒ writes `<dir>/<id>` (tmp+rename), then the blobs query below.
  3. First batch only: `ESC_tsp;q;{"q":"blobs","ids":[…]}ESC\`; terminal replies `ESC_tsp;r;{"r":"blobs","have":[ids]}ESC\`; omp sends those not in `have`. No reply in 3000 ms (`Gn`) ⇒ send all inline. Max 16 outstanding (`jn`). Later new blobs skip the query and are sent immediately.
  4. Inline blob: `ESC_tsp;b;id=<id>;mime=<mime>;<base64>ESC\` (chunked per §1.3) [backend@178560467 `#J`].
- Blobs are sent once per connection (`sent` set); reset on stop/resume (`#de`).

---

## 7. Overlays, sheets, pickers

### 7.1 How a TUI overlay becomes nodes
`TUI.overlays()` lists visible overlay components with their options [tui@178585580]. `NativeBackend.render` [backend@178554860]:
- Overlay whose component has `nativeSheet(cx) === true` ⇒ the component's own `describe()` node is placed **directly** in the `layer` root (no wrapper) [backend@178565067 `#F`].
- Else if component has `nativeOverlay` or is not `fullscreen` ⇒ wrapped: `s("overlay",{anchor,size,modal,role?,head?},[component],"ov-<compId>")` where `anchor` = `nativeOverlay.anchor ?? (options.anchor startsWith top→"top" | bottom→"bottom" | else "center")`; `size` = `nativeOverlay.size ?? (fullscreen→"full"; width ratio ≥0.75 "lg", ≥0.45 "md", else "sm")`; centre + terminal cols < 100 + (lg or md-nativeOverlay) ⇒ `"full"` (`eIt=100` [overlay@178521592]); `modal` = overlay focused or fullscreen. Nested overlays inside hoisted overlays are dropped with a log line.
- Fullscreen overlay with neither ⇒ separate `screen` surface (`describeScreen()` ?? `[component]` as `main`, `dock` from `describeScreen`).
- Focus: omp's focused component is mapped to its editable node ⇒ `["focus", id]` (§3.2). Close: omp removes the overlay component ⇒ `del` of the layer child (and `x` for screen surfaces). The terminal must close an overlay when the node disappears; the in-node `Close` buttons send `action act:"close"`.
- Shared overlay chrome [overlay@178521837…]: `cr(role, head, children)` = `card{role,head}`; hints row `Et([{keys,label}])` = `row{role:"omp.overlay.hints",gap:"md",wrap}` of `kbd`+muted text; button `ue(label, act, {keys,tone,title,href})` = `row{role:"omp.btn",gap:"xs",align:"center",actions:{click:act},title:"<label>  <keys>"}`; `jlt()` = Close `row{role:"omp.btn",actions:{click:"close"},title:"Close  esc"}[kbd esc]`; `tt([…])` = `row{role:"omp.actions",gap:"sm"}` with `null` ⇒ `spacer{grow:1}`; `hm(key, items, props)` = `list{actions:{click:"select",dblclick:"activate"}}`.

### 7.2 `picker` kind (sheet) [picker@179252484, XBe@179255564, JBe@179256529]
Props: `title subtitle icon noun size("lg"/"md") layout("rows"|"tree") preview("side"|"none") query(null=no search box|string) cursor placeholder tabs:[{id,label}] tab scopes:[{id,label,icon,group,count,disabled}] scope columns:[{id,head,format:"price|elapsed|num|bar|text|dim",priority,min}] items:[…] selected total state("ready|loading|error") message empty actions:[{id,label,keys:[…],primary,danger,disabled(true|reason string),on,end}] strip focus("list|scopes|strip") confirm current order`; **children** = preview/detail nodes (right pane). Item: `{id,label,detail,mono,dot(tone),chips:[{text,dot}],badges:[{text,tone,title}],facts:{<colId>:value},title,depth,open,icon,tone,disabled,group}`.
Events: `select`/`activate` (item id), `action act=<action id|tab|scope|strip|confirm|close|cancel>` with `value` for tab/scope/strip. `Ur()` normalises: `select|activate → {item}`, `action → {act,value}`; keys are matched after stripping `^` portal markers [picker@179254587]. Default `Qn = {id:"close",label:"Close",keys:["esc"],end:true}` [picker@179253187]. The sheet is only used if hello `kinds` contains `picker`; otherwise each overlay's non-picker variant is described (`card`-based).

### 7.3 Agent Hub (live subagent roster) - class `yle` [hub@189098303]
- Opened by `showAgentHub` → `showOverlay(component,{anchor:"bottom-center",width:"100%",maxHeight:"100%",margin:0,fullscreen:true})` + `setFocus` [sel@191143159 `#t`, @191198106]. Opened from Alt+A / Ctrl+S / double-← or the dock pill action `agents.open` [im@191213411].
- `nativeSheet(cx) = cx.supports("picker")` [hub@189104750] ⇒ with `picker`, the node sits directly in `layer` (fullscreen sheet); without it, `describe` yields `cr("omp.overlay.agentHub","Agent Hub",[tabs{items:[{id:"agents",label:"1 Agents"},{id:"activity",label:"2 Activity"}],active},…])` and, being fullscreen with no `nativeSheet`/`nativeOverlay`, becomes a `screen` surface [hub@189104806, @189116531 `#Zt`, @189125393 `#re`].
- **picker variant** `#Dt` [hub@189108833]: `vo({title:"Agents", subtitle:<"$cost · time · N req · N tok"|"No usage reported yet">, icon:"users", size:"lg", tabs:[{id:"agents",label:"Agents"},{id:"activity",label:"Activity"}] [hub@189095480], tab, …}, children)`.
  - **Agents tab** `#Gt` [hub@189109427]: `noun:"agents"`, `layout: "tree"|"rows"` (view toggle), `preview:"side"`, `query/cursor` (search box shown only while filtering or non-empty), `placeholder:"Filter agents…"`, **columns** `Cost(price,prio 6) Time(elapsed,5) Req(num,2) Tools(num,1) Tok(num,3) Ctx(bar,4)` [hub@189095571], `items`, `selected`, `total`, `state:"loading"` while persisted agents load, `empty:"No agents in this session. Finished, parked and killed subagents stay with the session that created them."`, `focus:"list"`, **actions**: `open "Open transcript" [enter] primary` (disabled if none), `revive "Revive" [r]` (disabled reason "Only parked or killed agents can be revived" unless status parked/aborted; "Read-only advisor transcript" for advisors), `kill "Kill" [x] danger`, `view "By parent" [t] on:(view==="tree")`, `filter "Filter" [/]`, `close "Close" [esc] end`.
  - **Item** `#Yt` [hub@189110809]: `{id, label:<agent id>, mono:true, dot:<pending|success|muted|error by status running|idle|parked|aborted> (zFe), detail:<task text, "Complete assignment thoroughly:" prefix stripped, [hubren@178957221]>, chips:[{text:<model>,dot:<thinking colour>}], badges:[{text:<model role>,title:"Model role"},{text:"read-only",tone:"warning"},{text:"N unread",tone:"warning"},{text:"↳ <parent>",tone:"muted",title:"Spawned by"}] (≤3), facts:{cost:"$0.012",time:<ms|short>,req,tools,tok,ctx:0..1}, title:"Context X / Y"}`; tree view adds `depth` and `open`. Cost format `$d`: <$0.01 → 4 dp, <$1 → 3 dp, else 2 dp [hubren@178960140].
  - **Detail pane (children)** `#Jt` [hub@189112611]: `row[title(text role omp.picker.title + badge{text:status,tone})]`, optional error `text{tone:"error"}`, **kv grid** (`role omp.hub.kv`) rows `Task, Model(fallback → model · level · role), Current (tool+args, retry n/m), Usage ("$ · time · N req · N tools · N tok"), Lineage ("Spawned by …· N children"), Children, Registered, Changes, Output(path, href file://), Patch, Nested patch, Worktree branch` [hub@189123034 `#Ct`], context `meter{value,style:"bar",size:"md",label:"X / Y · P%",tone}` (or `progress` without `meter`), `section{head:"Recent activity",role:"omp.hub.activity"}` of `row{role:"omp.hub.activity.row",actions:{click:"transcript"},title:"Open in transcript"}[time mono, icon, tool name, summary]` (last 20; each keyed by hash of entry id).
  - **Activity tab** `#Qt` [hub@189115000]: `noun:"events"`, `layout:"rows"`, `preview:"none"`, `placeholder:"Search activity…"`, `scopes:[All(list),Errors(x-circle),Responses(message),Tools(wrench)]` (group "Show") [hub@189096129], `scope`, columns `Agent(text,2) Time(dim,1)` [hub@189095946], items `#Xt` `{id,label:<tool/title>,mono,detail:<summary>,icon:message|wrench|send|activity,dot:"pending",tone,facts:{agent,at}}`, actions `open "Open transcript" [enter]`, `follow "Follow" [space] on`, `agents "<All agents|selected|subtree>" [s]`, `filter "Search" [/]`, `close`.
- **Events → effects** `handleNativeEvent` [hub@189105361], picker key `""` ⇒ `#jt` [hub@189106981]:
  - `select`/`activate` on item (agents tab): selects row; **`activate` ⇒ `#q(agent)`** = "Open transcript" [hub@189149990]. Activity tab: `activate` ⇒ `openChat(agentId, entryId)`; `select` pauses follow.
  - `action act:"tab" value:"agents"|"activity"` switches tab; `act:"scope" value∈{all,errors,responses,tools}`; `act:"close"` → `onDone()` (hub closes); `act:"filter"` opens the search input.
  - agents tab actions: `open` ⇒ `#q` (above); **`revive` ⇒ `#_t`** → `lifecycle().ensureLive(id)` (or `remote.revive(id)` for collab guests) if status is parked/aborted (else sets a red notice); **`kill` ⇒ `#Wt`** → if running `session.abort({reason})` then `lifecycle().release(id,agent,{tombstone:true})` (or `remote.kill(id)`); advisor rows refuse both with a notice; `view` toggles flat/tree.
  - activity tab actions: `open` (opens selected entry), `follow`, `agents` (cycle scope subject).
  - detail row `action act:"transcript"` ⇒ `openChat(agentId, entryId)` using the row key mapped in `#ot`.
  - **`#q`** (open): for local non-advisor agents with `focusAgent` → `ctx.focusAgentSession(id)` then close the hub (composer then shows a "Viewing <agent>" strip, §8); advisor/remote → `openChat` (fullscreen transcript viewer). Errors are shown as `text{tone:"error"}` notice.
  - `openChat` pushes another fullscreen overlay `fEe` (transcript viewer) whose describe is `cr("omp.hub.transcript",[Agent Hub · <id>],[row meta(id,badge status,kind · of parent) + Close btn, transcript col, notice, input, stats row (progress ctx / tools / time / cost), hints])`; its only native event is `action close` [tv@189087330].
- Non-picker fallback events: `select/activate` with key suffix `section` (item agents|activity), `view` (roster|tree), `agents` (row), `activity`, `recent`; `action` with `act:"transcript"|"tab"|"scope"|"close"|"filter"|"open"|"revive"|"kill"|"view"|"follow"|"agents"`.
- `/agents` (`agents-hub.ts`, `hubs`) is a **different** hub (agent *definitions*: scopes Source{Project,User,Bundled}, columns Model/Prewalk/Advisor/Source, actions configure/toggle/model/new/reload/close) [hubs@189179353]; `agents-hub-deps.ts` is its dependency bag, not the roster.

### 7.4 Jobs panel - class `aEe` [jobs@188316356]
- Opened by `showJobsSheet()` (`/jobs` or pill action `jobs.open`): `showOverlay(panel,{anchor:"center",width:"90%",maxHeight:"90%"})`, 1 s refresh timer; panel `cancel` callback is withheld when a subagent is focused (`focusedAgentId`) [im@191369620].
- Has `nativeOverlay = {role:"omp.overlay.jobs", size:"lg", anchor:"center", head:"Background jobs"}` and **no** `nativeSheet` ⇒ always wrapped in `overlay{anchor:"center",size:"lg",modal:true,role:"omp.overlay.jobs",head:"Background jobs"}` [jobs@188316553].
- Tree [jobs@188316553 `describe`]:
  - non-empty: `col[ row{role:"omp.jobs.split"}[ col{role:"omp.jobs.master"}[ list{key:"jobs",selected:<jobId>,max:{lines:12},actions:{click:"select",dblclick:"activate"}} of items ] , col{role:"omp.jobs.detail"} ], row{role:"omp.actions"}[ text{role:"omp.jobs.summary", "N running · M recent"}, spacer, btn Close(esc) ] ]`.
  - **master item** `E` [jobs@188321293]: `item{key:<jobId>, label:[tree-prefix mono dim, "<agentId> " strong, label (mono unless type=task)], value:[elapsed mono], icon: bash→"bash" task→"agent" else "job", tone: running→pending completed→success failed→error cancelled→muted, title:"<label>\n<Status> · <type> · <id>", aria}`; sub-agent grouping rows `text{role:"omp.jobs.group",key:"agent-<hash>"}`; status words Running/Done/Failed/Cancelled.
  - **detail** `A` [jobs@188322399]: `row{role:"omp.jobs.status",gap:"sm"}[ text{role:"omp.jobs.state",tone,spans:[glyph ●✓✕⊘ (running has fx:"pulse"), " <Status>"]}, elapsed{age,stopped?,format:"short",role:"omp.jobs.age",title:"Running for"|"Ran for"}, text{role:"omp.jobs.id","<type> · <id>"}, spacer, **btn "Cancel job" [x] tone:"error" (act `cancel`) only if running && cancellable** ]`, then `code{lang:"bash",wrap,preview:{lines:4},role:"omp.jobs.command"}` (or `text{role:"omp.jobs.title"}` for tasks), `kv{role:"omp.jobs.facts"}` (Agent, Directory, PID/PIDs, Exit code, Full output `artifact://<id>`), optional job widget `col{role:"omp.jobs.widget"}`, `col{role:"omp.jobs.out"}[text "Output", ansi{follow:!done,role:"omp.jobs.output",max:{h:"16lines"}} | text "No output"/"No output yet"]`.
  - empty: `col{role:"omp.jobs.empty",align:"center"}[icon{name:"jobs"}, text title "No background jobs", note "Commands and subagent tasks started in the background show up here while they run."]` + actions row `[spacer, Close]`.
- Events [jobs@188319379]: `select`/`activate` ⇒ selected job id = `item`; `action close` ⇒ `close()`; **`action cancel` ⇒ `cancel(jobId)` = `session.cancelAsyncJob(id)`** if running & cancellable (`x` key does the same via `handleInput`). No other events.

### 7.5 Other sheets using `picker`/`prefs`
model-hub (`/model`), session-selector, settings-selector (`prefs`), advisor-config (`prefs`), form lists > threshold, extension-dashboard, copy-selector - all `nativeSheet = supports("picker")` [model-hub@185188705, form@179479419].

---

## 8. Composer / dock

`Composer.describeSurface()` returns `{main, dock}` [composer@184956759]: `main` = transcript container children (+ blocks), `dock` = `nativeDock` list when set, else `[editor]`. interactive-mode installs [im@191246421]:
`dock = [HudPills(a_), btwContainer, omfgContainer, cleanseContainer, errorBannerContainer, modelCycleContainer, deferredCommandContainer, progressHudContainer, statusContainer, pendingMessagesContainer, hookWidgetAbove, editorContainer(editor + attachment chips), hookWidgetBelow]`.

### 8.1 Editor (`CustomEditor.describe` → `Editor.describe`) [ce@180179746, ed@179399521]
`col{role:"omp.editor" | "omp.editor.bash|python", tone:"pending" while running, children:[context meter?, viewing strip?, attachment chips, row{role:"omp.composer.line"}[mode chip?, editor{key:"input"}], row{role:"omp.composer.bar"}[model, effort, rate, extras, usage, dictation, submit]], + overlay{anchor:{caret:"line/input"},role:"omp.autocomplete"}[select list] }` [ce@180182697 `describeLayout`]. Caret anchor ids are re-based to the component by the reconciler.
`editor` props: `text cursor sendable anchor? decor:[{from,to,s,fx}] ghost placeholder(string|spans: "Message <agent>" / "“title”"/default) mode(vim) lang(bash|python) maxLines`; `decor` styles: `hide`, `customMessageLabel strong` (skills), `statusLineModel strong` (mentions), `accent strong` (refs), `accent` (+`fx:"shimmer"` for magic keywords), `dim` (`->`), `typo`.

### 8.2 Bar controls [ce@180185459 `#B`]
- Model: `row{role:"omp.composer.model",tone,title:"Switch model  <key>",actions:{click:"status.model"}}[effort|icon "model", text{spans:model.spans}, icon "chev"]` → action `status.model` opens the temporary model selector.
- Effort: `effort{level,role:"omp.composer.model.effort",title:"Thinking effort: <lvl>  <key>",actions:{click:"thinking.cycle"}}` (when hello has `effort`), else `row{role:"omp.composer.effort",title:"Thinking effort…"}[meter{style:"blocks",steps:4}|effort, text level]`.
- Rate `rate{value,unit:"tok/s",role:"omp.composer.rate",title:"Generation rate"}`; dictation row (`dictation.toggle`, icons mic/stop, `spinner` "Starting…/Transcribing…", tone error/pending); submit `kbd{keys:["enter"],role:"omp.composer.send",title:"Send  enter",actions:{click:"submit"}}` or running `text{role:"omp.composer.stop","Stop",tone:"error",actions:{click:"interrupt"}}`.
- Viewing strip when focused on a subagent `row{role:"omp.composer.focus",title:"Viewing subagent <id>: what you send goes to it"}[icon eye, "Viewing", crumbs (click → action `focus:<id>`), agent, row{role:"omp.composer.exit",title:"Back to the main session  <key>",actions:{click:"focus:<mainId>"}}[kbd,"main"]]`.
- Facts [sl@184928891 `describeComposerFacts`]: `meter{role:"omp.composer.context",value,style:"bar",thresholds,parts?,marks?,label:"NN%",total:"200k",title:"Context 42% used\nX of Y tokens\nAuto-compact at 80%…",actions:{click:"status.context"}}`; `text{role:"omp.composer.usage",text:"$1.23",title:"Session cost $1.23",actions:{click:"status.cost"}}`; extras `status{role:"omp.composer.extras",transparent:true,grow:1}` of `seg{role:"omp.composer.fact",priority,spans,icon,tone,title,actions:{click:"status.git"|"status.path"}}` [sl@184863432 `we`]. omp maps clicks `status.model|context|git|cost|path` in `onNativeAction` [im@191239335].
- Attachment chips: `row{role:"omp.composer.chips",wrap,hidden:when none}` of `card{role:"omp.composer.chip",tone:"accent",head:"#n"}` with `image{max:{w:"12ch",h:"4lines"}}` or paste preview text [chips@188253989].

### 8.3 Subagent / jobs pills (HUD)
`HudPills(a_)` [im@191213411] describes `describeHudPills()` [im@191227800]: `row{role:"omp.hud",justify:"end",hidden:when empty}[row{role:"omp.hud.pill",key:"running"}[agents pill, jobs pill]]`. Pill builders:
- **Agents** (`l_`) [im@191215600]: `row{role:"omp.hud.seg",gap:"xs",align:"center",title:"<N> agent(s) running  <shortcut>",aria:"N agents running",actions:{click:"agents.open"}}[icon{name:"users",tone:"accent"}, text{spans:[{t:"N","s":"num"}],wrap:"none"}]`. When 0 running but some in session (`tZ` [im@191214040]): `title:"No agents running · N in this session  <shortcut>"`, only the muted icon. Hidden if setting `display.pinnedAgents` is `off`.
- **Jobs** (`rZ`/`uZ`/`l_`) [im@191214775]: `row{role:"omp.hud.seg",title:"N job(s) running  /jobs\n<jobId>  <widget title - label value trend>…",aria,actions:{click:"jobs.open"},hovercard:<job tiles subtree>}[icon{name:"job",tone:"warning"}, text{N,"num"}, text{role:"omp.hud.glance",spans}?, chart{role:"omp.hud.spark",kind:"spark",size:"sm"}?]`. `hovercard` = `col{role:"omp.jobs.hovercard",gap:"md"}[row{role:"omp.job.tiles",wrap}[tile…(≤4)], text "+N more jobs · /jobs"]`, tile = `col{role:"omp.job.tile",tone}`[`row head`: dot (● ✓ ✕ ■, running fx:"pulse"), title, id, `meter{style:"ring",size:"sm"}`, `elapsed`; figure; chart/visual] [job@180098827, @180100040].
- `a_.handleNativeEvent`: `action agents.open` → `showAgentHub()`; `action jobs.open` → `showJobsSheet()`.
- The status-line module also builds `seg`s `subagent-badge` (`icon:"agents"`) and `jobs` (`icon:"job"`) in the *preview/status* describe (`#jt`), but the live dock uses the pills above [sl@184929886 region].
- `statusContainer` is wrapped in `describeStatusHud`: `col{role:"omp.hud.status"}` + `checklist{mode:"hud",role:"omp.hud.todo"}` (if hello has `checklist`) [im@191228707].

---

## 9. Resize, scrolling, selection - who does what

From omp docs and code: with a live surface *"the terminal owns layout and scrolling"*; none of omp's history/viewport/resize-replay/CPR machinery runs; SIGWINCH only refreshes `cols` used by `rows` fallback nodes; omp does **not** reflow transcripts (docs: tui-runtime-internals § Native surfaces; code `noteTerminalColumns` [tui@178584075]).

| concern | terminal (Mkrew) | omp |
|---|---|---|
| layout (flex, wrap, truncate, `min/max` units `ch`/`lines`) | **all** | none (except `rows` fallback at `cols`) |
| scrolling of `main`, stick-to-bottom, `reveal`/`scroll` ops, per-node `max.h` scroll, `ansi.follow` | **all** | emits `reveal`/`scroll` hints only |
| text selection, copy, hyperlinks (`href`), context menu (`actions.menu`) | **all** (no wire event for them; copy of editor text happens locally) | none (copy/export actions are explicit `action` events like `copy`) |
| editor text editing, cursor, undo | terminal may (a) forward keys to PTY (omp edits) or (b) edit locally and send `edit`/`undo`/`send`; `sendable` gate | applies `edit` via `applyHostEdit`; owns the draft |
| collapse/expand (`card`/`section` `collapsible`) | local state + `toggle` event | stores `collapsed` and re-describes |
| focus | shows focus on `["focus",id]`; sends `focus{id}` on pointer click | decides focused component |
| theme | applies `t` tokens, answers OSC 11 so omp can derive dark/light, sends `theme`/`motion` | forces nerd symbols; re-sends `t` on palette change |
| width for `rows` | `resize{cols}` (and hello `cols`) | re-renders `rows` at that width |
| animation (`spinner`, `elapsed`, `shimmer`, `fx:pulse`) | local timers | static props |
| images | decode/cache blobs, answer blobs query | encode, hash, chunk |
| pacing | `ack` every frame | sends ≤ `credits` frames in flight |
| overlays | place by `anchor/size/modal`, close on node removal | owns lifecycle |

---

## 10. Size of the job; unknown kinds and the `rows` fallback

### 10.1 Unknown kinds / rows fallback (omp side)
- omp **never filters nodes by the terminal's `kinds`**: `reconcile.ts` contains no `supports()` call; only `feature("scroll")` [reconcile@178529465]. Components that care test `cx.supports(kind)` themselves and pick another variant (`picker→card/list`, `tree→list`, `meter→progress`, `effort→meter blocks`, `tool→card`, `checklist→fallback`, `media→image`).
  Kinds with no such check (`chart agent prefs toast math diff shimmer rate seg status …`) are sent regardless of hello. [INFERENCE] a terminal should render unknown kinds by drawing their children (or nothing) rather than failing.
- `rows` is chosen **per component**, not per kind: `Reconciler.#J` [reconcile@178544240] calls `comp.describe(cx)`; if the component has no `describe`, or has the default `describe` while overriding `render`, or `describe` returns falsy, it counts `fallbackCount++`, calls `render(max(1,cx.cols))` and emits `{k:"rows",p:{cols,lines:[…]}}` (lines are raw ANSI strings; an internal cursor marker is stripped). Re-sent only when the lines array or `cols` changes. The terminal must therefore be able to draw an ANSI line grid in a node (SGR colours/attrs, OSC 8 links, kitty/sixel images are not special-cased).
- omp's own reference document (`Glt`) rejects kinds not in `oUe` (`unknown kind X`) - used only for diagnostics.
- If the whole handshake fails → rows renderer for everything (classic ANSI TUI) [tui@178584764].

### 10.2 Minimal working surface (everything needed to show omp usable)
1. **Transport**: APC `tsp` scanner on PTY output (strip from grid; ST terminator; reassemble `c`/`m` chunks); APC writer to PTY input (≤ `apc` bytes per body; JSON; ST); optional OSC 877 variant.
2. **Handshake**: answer `q:hello` with `r:hello` (v:1, term, kinds, features, apc, credits, cols, dark) **before** the paired DA1 reply; also answer plain DA1/DA-style probes (`ESC[?u`, `$p`, OSC 11) correctly so omp's other sentinels don't wedge. Set `TERM_PROGRAM=tern` (optimistic) or `PI_TUI_NATIVE=1` in the child env.
3. **Session**: handle `o` (inline; `screen` if you advertise it), `x`, `t` (token→colour map), `f` (+ `ack` per frame; `resize`).
4. **Document model** with ops add/set/text/splice/move/del/focus/settle (reveal/scroll may be no-ops initially) keyed by node id; root `main/dock/layer` under the surface.
5. **Renderer** for the kinds omp uses constantly: `col row text(+spans) md code ansi card section rule spacer kv table list item tabs badge kbd icon(+PUA glyph font) spinner elapsed progress seg status image(+blobs) tool agent(or card fallback) overlay editor input rows`; **omit** `picker tree meter tool effort checklist media` from `kinds` to get card/list/progress variants for the heavy ones.
6. **Interaction**: `actions.click/dblclick` → `action`/`select`/`activate`; `toggle`; `focus`; and either PTY key forwarding (works day-1) or `edit`/`send` handling; `error` on bad ops.
7. **Overlay host** (modal layering, close on node removal; `Esc` via PTY keys) - needed for Jobs/Agent Hub fallback cards.
Result for Agent Hub w/o `picker`: `screen` surface with `tabs` + `list`/`tree`-less roster (`list` of `item`s) + `kv` detail; buttons are only the keyboard hints (Revive/Kill need keys `r`/`x` through the PTY because the fallback has no action buttons) [hub@189116531 `#Zt`]. Jobs panel: complete (Cancel job button + `x`).

### 10.3 Full parity additions
- `picker` sheets with preview split, scopes, tabs, columns with formats (`price|elapsed|num|bar`), tree layout, actions row with `primary/danger/disabled/on`, search box state sync (`query/cursor`) (Agent Hub, `/model`, `/agents`, settings, sessions).
- `meter` (bar/ring/blocks, thresholds, parts, marks, `total`), `chart spark`, `effort` glyph, `checklist` (full/reminder/hud), `tool`, `agent`, `diff` (unified/split/hunks), `math`, `media`, `toast`, `shimmer`, `rate`, `prefs`.
- `hovercard` on pills; `title` tooltips; `aria`; `actions.menu`; theme `role` styling for the 485 `omp.*` roles; `settle` caching; `scroll` feature + `reveal`; `gone` eviction; `blobs` cache (`TERN_BLOB_DIR`); `edit/undo/send` local text editing with `sendable` gating; reduced-motion; OSC 11 / theme events; per-surface multiple (`screen`).
- Rough size [INFERENCE]: transport+handshake+doc model ≈ small (1–2 days); core ~20 kinds + overlays ≈ medium; picker/tool/meter/chart/diff/editor-parity + role theming ≈ the bulk (weeks).

### 10.4 Mkrew-specific notes [INFERENCE]
- The UI-side emulator must see the APC in PTY output (daemon must pass bytes through unmodified) and inject replies via the daemon into the PTY input; ConPTY behaviour for APC both ways must be tested early (§1.5).
- Because omp keeps processing raw keys, a first milestone can ship render-only TSP (hello + frames + acks) while keyboard stays on the PTY; Agent Hub/Jobs then work via keys, buttons via `action` events later.
