use base64::Engine as _;

use super::*;
use crate::tsp::fixture;

const SF: &str = "s:1";

fn apc(verb: u8, body: Value) -> Vec<u8> {
    wire::encode(verb, &body, wire::MAX_MESSAGE).unwrap()
}

fn hello_query(versions: Value) -> Vec<u8> {
    apc(
        b'q',
        json!({"q": "hello", "v": versions, "app": "omp", "features": ["edit", "undo", "send"]}),
    )
}

fn open_bytes() -> Vec<u8> {
    apc(
        b'o',
        json!({"id": SF, "mode": "inline", "title": "omp", "role": "omp.session"}),
    )
}

fn frame_bytes(sf: &str, s: u64, ops: Value) -> Vec<u8> {
    apc(b'f', json!({"sf": sf, "s": s, "ops": ops}))
}

fn main_op() -> Value {
    json!(["add", "main", SF, null, {"id": "main", "k": "col"}])
}

/// Verb and JSON body of one encoded message.
fn parse(bytes: &[u8]) -> (u8, Value) {
    let mut pieces = Splitter::new().feed(bytes);
    assert_eq!(pieces.len(), 1, "{bytes:?}");
    let Piece::Message(raw) = pieces.remove(0) else {
        panic!("not a message: {bytes:?}")
    };
    (raw.verb, serde_json::from_slice(&raw.body).unwrap())
}

fn reply_bodies(steps: &[Step]) -> Vec<Value> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::Reply(b) => Some(parse(b).1),
            _ => None,
        })
        .collect()
}

/// Joins neighbouring `Plain` steps (a splitter may cut plain bytes at a chunk edge).
fn merged(steps: Vec<Step>) -> Vec<Step> {
    let mut out: Vec<Step> = Vec::new();
    for step in steps {
        if let (Step::Plain(add), Some(Step::Plain(last))) = (&step, out.last_mut()) {
            last.extend_from_slice(add);
            continue;
        }
        out.push(step);
    }
    out
}

fn ack(sf: &str, s: u64) -> Value {
    json!({"ev": "ack", "sf": sf, "s": s})
}

/// An engine that answered the hello and opened `s:1`.
fn live() -> (Engine, Instant) {
    let t = Instant::now();
    let mut e = Engine::new(120, true);
    e.on_output(t, &hello_query(json!([1])));
    e.on_output(t, &open_bytes());
    (e, t)
}

fn child_ids(node: &TspNode) -> Vec<&str> {
    node.c.iter().map(|n| n.id.as_str()).collect()
}

#[test]
fn the_hello_reply_has_exactly_the_contract_fields() {
    let mut e = Engine::new(120, false);
    let steps = e.on_output(Instant::now(), &hello_query(json!([1])));
    let [Step::Reply(bytes)] = steps.as_slice() else {
        panic!("{steps:?}")
    };
    let (verb, body) = parse(bytes);
    assert_eq!(verb, b'r');
    assert_eq!(
        body,
        json!({
            "r": "hello", "v": 1, "term": "mkrew", "kinds": P1_KINDS, "features": [],
            "apc": 65536, "credits": 2, "cols": 120, "dark": false,
            "reduceMotion": false, "hour12": false
        })
    );
    assert_eq!(e.state(), TspState::Waiting);
}

#[test]
fn the_hello_reply_comes_before_the_da1_bytes_at_every_chunk_split() {
    let mut stream = hello_query(json!([1]));
    stream.extend_from_slice(b"\x1b[c");
    for cut in 0..=stream.len() {
        let t = Instant::now();
        let mut e = Engine::new(120, true);
        let mut steps = e.on_output(t, &stream[..cut]);
        steps.extend(e.on_output(t, &stream[cut..]));
        let steps = merged(steps);
        assert!(
            matches!(steps.as_slice(), [Step::Reply(r), Step::Plain(p)]
                if parse(r).1["r"] == "hello" && p[..] == b"\x1b[c"[..]),
            "cut {cut}: {steps:?}"
        );
    }
}

#[test]
fn an_unsupported_hello_version_falls_back_without_a_reply() {
    let mut e = Engine::new(120, true);
    let steps = e.on_output(Instant::now(), &hello_query(json!([2, 3])));
    assert!(
        matches!(
            steps.as_slice(),
            [Step::State(TspState::Fallback), Step::Warn(_)]
        ),
        "{steps:?}"
    );
    assert_eq!(e.state(), TspState::Fallback);
    assert_eq!(e.deadline(), None);

    // omp may offer several versions; `1` among them is enough.
    let mut e = Engine::new(120, true);
    let steps = e.on_output(Instant::now(), &hello_query(json!([2, 1])));
    assert_eq!(reply_bodies(&steps).len(), 1);
}

#[test]
fn the_deadline_runs_from_the_first_output_until_a_hello_is_answered() {
    let t = Instant::now();
    let mut e = Engine::new(120, true);
    assert_eq!(e.deadline(), None, "nothing was output yet");

    e.on_output(t, b"plain text");
    assert_eq!(e.deadline(), Some(t + HELLO_WAIT));
    e.on_output(t + Duration::from_secs(1), b"more text");
    assert_eq!(
        e.deadline(),
        Some(t + HELLO_WAIT),
        "later output does not move it"
    );

    assert!(
        e.on_deadline(t + HELLO_WAIT - Duration::from_millis(1))
            .is_empty()
    );
    assert_eq!(e.state(), TspState::Waiting);
    let steps = e.on_deadline(t + HELLO_WAIT);
    assert!(
        matches!(
            steps.as_slice(),
            [Step::State(TspState::Fallback), Step::Warn(_)]
        ),
        "{steps:?}"
    );
    assert_eq!(e.state(), TspState::Fallback);
    assert_eq!(e.deadline(), None);
    assert_eq!(
        e.snapshot().reason.as_deref(),
        Some("no TSP reply from the terminal layer"),
        "the UI toasts this reason"
    );

    let mut answered = Engine::new(120, true);
    answered.on_output(t, &hello_query(json!([1])));
    assert_eq!(answered.deadline(), None);
    assert!(answered.on_deadline(t + Duration::from_secs(60)).is_empty());
    assert_eq!(answered.state(), TspState::Waiting);
}

#[test]
fn a_hello_after_the_fall_back_passes_through_unanswered() {
    let t = Instant::now();
    let mut e = Engine::new(120, true);
    e.on_output(t, b"rows");
    e.on_deadline(t + HELLO_WAIT);
    let hello = hello_query(json!([1]));
    assert_eq!(
        e.on_output(t + HELLO_WAIT, &hello),
        vec![Step::Plain(hello)]
    );
}

#[test]
fn open_then_frame_updates_clients_and_acks_with_the_frame_seq() {
    let t = Instant::now();
    let mut e = Engine::new(120, true);
    e.on_output(t, &hello_query(json!([1])));

    let steps = e.on_output(t, &open_bytes());
    assert_eq!(steps.len(), 2, "{steps:?}");
    let Step::Update(TspChangeFor {
        surface,
        change: TspChange::Open { surface: s },
    }) = &steps[0]
    else {
        panic!("{steps:?}")
    };
    assert_eq!(surface, SF);
    assert_eq!(
        (
            s.id.as_str(),
            s.mode.as_str(),
            s.role.as_deref(),
            s.title.as_deref(),
            s.seq,
            s.read_only
        ),
        (SF, "inline", Some("omp.session"), Some("omp"), 0, false)
    );
    assert_eq!(s.root.id, SF);
    assert_eq!(steps[1], Step::State(TspState::Live));
    assert_eq!(e.state(), TspState::Live);

    let steps = e.on_output(t, &frame_bytes(SF, 1, json!([main_op()])));
    assert_eq!(steps.len(), 2, "{steps:?}");
    assert_eq!(
        steps[0],
        Step::Update(TspChangeFor {
            surface: SF.into(),
            change: TspChange::Frame {
                seq: 1,
                ops: vec![main_op()],
                blobs: vec![]
            },
        })
    );
    let Step::Reply(bytes) = &steps[1] else {
        panic!("{steps:?}")
    };
    assert_eq!(parse(bytes), (b'e', ack(SF, 1)));
}

#[test]
fn a_frame_with_a_bad_op_forwards_the_good_ops_then_reports_and_acks() {
    let (mut e, t) = live();
    let good_add =
        json!(["add", "t1", "main", null, {"id": "t1", "k": "text", "p": {"text": "hi"}}]);
    let ops = json!([main_op(), ["del", "nope"], good_add]);
    let steps = e.on_output(t, &frame_bytes(SF, 1, ops));
    assert_eq!(steps.len(), 4, "{steps:?}");
    assert_eq!(
        steps[0],
        Step::Update(TspChangeFor {
            surface: SF.into(),
            change: TspChange::Frame {
                seq: 1,
                ops: vec![main_op(), good_add],
                blobs: vec![]
            },
        })
    );
    let Step::Reply(err) = &steps[1] else {
        panic!("{steps:?}")
    };
    let (verb, body) = parse(err);
    assert_eq!((verb, &body["ev"]), (b'e', &json!("error")));
    assert!(
        body["msg"].as_str().unwrap().starts_with("frame 1: "),
        "{body}"
    );
    assert!(matches!(&steps[2], Step::Warn(_)), "{steps:?}");
    let Step::Reply(a) = &steps[3] else {
        panic!("{steps:?}")
    };
    assert_eq!(parse(a).1, ack(SF, 1));
}

#[test]
fn a_frame_for_an_unknown_surface_is_skipped_with_a_warning_but_acked() {
    let (mut e, t) = live();
    let steps = e.on_output(t, &frame_bytes("s:9", 3, json!([main_op()])));
    assert_eq!(steps.len(), 2, "{steps:?}");
    assert!(matches!(&steps[0], Step::Warn(_)), "{steps:?}");
    let Step::Reply(a) = &steps[1] else {
        panic!("{steps:?}")
    };
    assert_eq!(parse(a).1, ack("s:9", 3));
    assert_eq!(e.snapshot().surfaces[0].seq, 0, "s:1 is untouched");
}

#[test]
fn reopening_a_surface_replaces_its_document_and_restarts_seq() {
    let (mut e, t) = live();
    let text = json!(["add", "t1", "main", null, {"id": "t1", "k": "text", "p": {"text": "old"}}]);
    e.on_output(t, &frame_bytes(SF, 1, json!([main_op(), text])));
    e.on_output(
        t,
        &frame_bytes(SF, 2, json!([["set", "t1", {"tone": "accent"}]])),
    );
    assert_eq!(e.snapshot().surfaces[0].seq, 2);

    let again = apc(
        b'o',
        json!({"id": SF, "mode": "inline", "title": "again", "role": "omp.session"}),
    );
    let steps = e.on_output(t, &again);
    assert_eq!(steps.len(), 1, "no State step: already live: {steps:?}");
    let Step::Update(TspChangeFor {
        change: TspChange::Open { surface },
        ..
    }) = &steps[0]
    else {
        panic!("{steps:?}")
    };
    assert_eq!((surface.seq, surface.title.as_deref()), (0, Some("again")));
    assert!(surface.root.c.is_empty(), "clients replace, not merge");
    let snap = e.snapshot();
    assert_eq!(snap.surfaces.len(), 1);
    assert!(snap.surfaces[0].root.c.is_empty());

    // A merged document would reject this `main` as a duplicate id.
    let steps = e.on_output(t, &frame_bytes(SF, 1, json!([main_op()])));
    assert_eq!(reply_bodies(&steps), vec![ack(SF, 1)], "{steps:?}");
}

#[test]
fn closing_the_inline_surface_without_keep_falls_back() {
    let (mut e, t) = live();
    e.on_output(t, &frame_bytes(SF, 1, json!([main_op()])));
    let steps = e.on_output(t, &apc(b'x', json!({"id": SF, "keep": false})));
    assert!(
        matches!(
            steps.as_slice(),
            [
                Step::Update(TspChangeFor {
                    change: TspChange::Closed { keep: false },
                    ..
                }),
                Step::State(TspState::Fallback),
                Step::Warn(_)
            ]
        ),
        "{steps:?}"
    );
    assert_eq!(e.state(), TspState::Fallback);
    assert!(e.snapshot().surfaces.is_empty());
}

#[test]
fn closing_with_keep_leaves_a_read_only_surface_in_the_snapshot() {
    let (mut e, t) = live();
    e.on_output(t, &frame_bytes(SF, 1, json!([main_op()])));
    let steps = e.on_output(t, &apc(b'x', json!({"id": SF, "keep": true})));
    assert_eq!(
        steps,
        vec![Step::Update(TspChangeFor {
            surface: SF.into(),
            change: TspChange::Closed { keep: true }
        })]
    );
    assert_eq!(e.state(), TspState::Live);
    let snap = e.snapshot();
    let s = &snap.surfaces[0];
    assert!(s.read_only);
    assert_eq!(s.seq, 1);
    assert_eq!(child_ids(&s.root), ["main"]);
}

#[test]
fn resize_is_sent_only_while_live_and_the_hello_uses_the_latest_cols() {
    let t = Instant::now();
    let mut e = Engine::new(120, true);
    assert_eq!(e.resize(90), None, "not live yet");
    let steps = e.on_output(t, &hello_query(json!([1])));
    assert_eq!(reply_bodies(&steps)[0]["cols"], json!(90));

    e.on_output(t, &open_bytes());
    let bytes = e.resize(100).expect("live");
    assert_eq!(parse(&bytes), (b'e', json!({"ev": "resize", "cols": 100})));

    e.on_output(t, &apc(b'x', json!({"id": SF, "keep": false})));
    assert_eq!(e.resize(80), None, "fallen back");
}

#[test]
fn events_outside_the_p1_list_are_rejected_and_sf_is_always_the_callers() {
    let (mut e, _) = live();
    let bytes = e
        .event(
            SF,
            json!({"ev": "action", "id": "dock.x", "act": "stop", "sf": "s:9"}),
        )
        .unwrap();
    assert_eq!(
        parse(&bytes),
        (
            b'e',
            json!({"ev": "action", "id": "dock.x", "act": "stop", "sf": SF})
        )
    );
    for bad in [
        json!({"ev": "edit", "id": "x"}),
        json!({"ev": "bogus"}),
        json!({"id": "x"}),
        json!("action"),
        json!({"ev": "theme", "dark": "yes"}),
    ] {
        assert!(e.event(SF, bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn a_theme_event_sets_dark_for_the_next_hello() {
    let mut e = Engine::new(80, true);
    let bytes = e.event(SF, json!({"ev": "theme", "dark": false})).unwrap();
    assert_eq!(
        parse(&bytes).1,
        json!({"ev": "theme", "dark": false, "sf": SF})
    );
    let steps = e.on_output(Instant::now(), &hello_query(json!([1])));
    assert_eq!(reply_bodies(&steps)[0]["dark"], json!(false));
}

#[test]
fn blobs_are_stored_and_attached_to_the_frames_that_name_them() {
    let (mut e, t) = live();
    e.on_output(t, &frame_bytes(SF, 1, json!([main_op()])));
    let png = b"PNGDATA";
    let blob = format!(
        "\x1b_tsp;b;id=abc;mime=image/png;{}\x1b\\",
        base64::engine::general_purpose::STANDARD.encode(png)
    );
    assert!(e.on_output(t, blob.as_bytes()).is_empty());
    let want = vec![TspBlob {
        id: "abc".into(),
        mime: "image/png".into(),
        data: png.to_vec(),
    }];

    let frame_blobs = |e: &mut Engine, seq: u64, ops: Value| -> Vec<TspBlob> {
        let steps = e.on_output(t, &frame_bytes(SF, seq, ops));
        let Step::Update(TspChangeFor {
            change: TspChange::Frame { blobs, .. },
            ..
        }) = &steps[0]
        else {
            panic!("{steps:?}")
        };
        blobs.clone()
    };

    let add = json!(["add", "img", "main", null, {"id": "img", "k": "image", "p": {"blob": "abc", "alt": "x"}}]);
    assert_eq!(
        frame_blobs(&mut e, 2, json!([add])),
        want,
        "add of an image node"
    );
    assert!(frame_blobs(&mut e, 3, json!([["set", "img", {"alt": "y"}]])).is_empty());
    assert_eq!(
        frame_blobs(&mut e, 4, json!([["set", "img", {"blob": "abc"}]])),
        want,
        "set of the blob prop"
    );
    let unknown =
        json!(["add", "img2", "main", null, {"id": "img2", "k": "image", "p": {"blob": "nope"}}]);
    assert!(
        frame_blobs(&mut e, 5, json!([unknown])).is_empty(),
        "unknown blob id"
    );
    assert_eq!(e.snapshot().blobs, want);
}

#[test]
fn the_oldest_blobs_are_dropped_past_the_byte_cap_and_the_snapshot_keeps_only_used_ones() {
    let (mut e, t) = live();
    e.max_blob_bytes = 10;
    let blob = |id: &str, n: usize| {
        let body = base64::engine::general_purpose::STANDARD.encode(vec![7u8; n]);
        format!("\x1b_tsp;b;id={id};mime=image/png;{body}\x1b\\")
    };
    let image =
        |id: &str| json!(["add", id, "main", null, {"id": id, "k": "image", "p": {"blob": id}}]);
    e.on_output(t, &frame_bytes(SF, 1, json!([main_op()])));
    for (id, n) in [("a", 4), ("b", 4), ("c", 4)] {
        e.on_output(t, blob(id, n).as_bytes());
    }
    // 12 bytes > 10: "a" was dropped, "b" and "c" remain.
    e.on_output(t, &frame_bytes(SF, 2, json!([image("a"), image("b")])));
    let ids = |e: &Engine| {
        e.snapshot()
            .blobs
            .into_iter()
            .map(|b| b.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ids(&e),
        ["b"],
        "a is gone; c is stored but no image names it"
    );
    e.on_output(t, &frame_bytes(SF, 3, json!([image("c")])));
    assert_eq!(ids(&e), ["b", "c"]);
}

#[test]
fn the_startup_fixture_replays_to_one_live_surface_with_every_frame_acked() {
    let t = Instant::now();
    let mut e = Engine::new(120, true);
    e.on_output(t, &hello_query(json!([1])));
    let lines = fixture::out_lines();
    let frames = lines.iter().filter(|(verb, _)| *verb == b'f').count() as u64;
    let mut acks = Vec::new();
    for (verb, body) in &lines {
        for step in e.on_output(t, &apc(*verb, body.clone())) {
            match step {
                Step::Warn(w) => panic!("the fixture must apply cleanly: {w}"),
                Step::Reply(bytes) => acks.push(parse(&bytes).1),
                _ => {}
            }
        }
    }
    assert!(frames > 0);
    assert_eq!(acks, (1..=frames).map(|s| ack(SF, s)).collect::<Vec<_>>());

    let snap = e.snapshot();
    assert_eq!(snap.state, TspState::Live);
    assert_eq!(snap.surfaces.len(), 1);
    let s = &snap.surfaces[0];
    assert_eq!((s.id.as_str(), s.seq), (SF, frames));
    assert_eq!(s.root.id, SF);
    assert_eq!(child_ids(&s.root), ["main", "dock", "layer"]);
}

/// An engine with a 12-node bound after a frame that evicted the oldest `main` rows; returns the evicted ids.
fn evicted() -> (Engine, Instant, Vec<String>) {
    let t = Instant::now();
    let mut e = Engine::with_limits(80, true, 12, MAX_TEXT);
    e.on_output(t, &hello_query(json!([1])));
    e.on_output(t, &open_bytes());
    let setup = json!([
        main_op(),
        ["add", "dock", SF, null, {"id": "dock", "k": "col"}],
        ["add", "layer", SF, null, {"id": "layer", "k": "col"}],
        ["add", "d1", "dock", null, {"id": "d1", "k": "text", "p": {"text": "composer"}}]
    ]);
    e.on_output(t, &frame_bytes(SF, 1, setup));
    let rows: Vec<Value> = (1..=10)
        .map(|i| json!(["add", format!("r{i}"), "main", null, {"id": format!("r{i}"), "k": "text", "p": {"text": "row"}}]))
        .collect();
    let steps = e.on_output(t, &frame_bytes(SF, 2, json!(rows)));
    let gone = reply_bodies(&steps)
        .into_iter()
        .find(|b| b["ev"] == "gone")
        .expect("a gone event");
    let ids = gone["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    (e, t, ids)
}

#[test]
fn eviction_reports_gone_and_forwards_the_dels_and_the_trim_row() {
    let t = Instant::now();
    let mut e = Engine::with_limits(80, true, 12, MAX_TEXT);
    e.on_output(t, &hello_query(json!([1])));
    e.on_output(t, &open_bytes());
    let setup = json!([
        main_op(),
        ["add", "dock", SF, null, {"id": "dock", "k": "col"}],
        ["add", "layer", SF, null, {"id": "layer", "k": "col"}],
        ["add", "d1", "dock", null, {"id": "d1", "k": "text", "p": {"text": "composer"}}]
    ]);
    e.on_output(t, &frame_bytes(SF, 1, setup));

    let rows: Vec<Value> = (1..=10)
        .map(|i| json!(["add", format!("r{i}"), "main", null, {"id": format!("r{i}"), "k": "text", "p": {"text": "row"}}]))
        .collect();
    let steps = e.on_output(t, &frame_bytes(SF, 2, json!(rows)));
    assert_eq!(steps.len(), 3, "update, gone, ack: {steps:?}");

    let Step::Reply(g) = &steps[1] else {
        panic!("{steps:?}")
    };
    let gone = parse(g).1;
    assert_eq!((&gone["ev"], &gone["sf"]), (&json!("gone"), &json!(SF)));
    let ids: Vec<String> = gone["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert!(!ids.is_empty());
    let oldest_first: Vec<String> = (1..=ids.len()).map(|i| format!("r{i}")).collect();
    assert_eq!(
        ids, oldest_first,
        "the oldest rows go, newest and dock stay"
    );
    let Step::Reply(a) = &steps[2] else {
        panic!("{steps:?}")
    };
    assert_eq!(parse(a).1, ack(SF, 2));

    let Step::Update(TspChangeFor {
        change: TspChange::Frame { seq, ops, .. },
        ..
    }) = &steps[0]
    else {
        panic!("{steps:?}")
    };
    assert_eq!(*seq, 2);
    assert_eq!(ops[..10], rows[..], "the frame's own ops come first");
    let extra = &ops[10..];
    assert_eq!(
        extra.len(),
        ids.len() + 1,
        "one del per removed id plus the trim row: {extra:?}"
    );
    let mut dels: Vec<String> = extra
        .iter()
        .filter(|op| op[0] == "del")
        .map(|op| op[1].as_str().unwrap().to_owned())
        .collect();
    dels.sort();
    let mut sorted_ids = ids.clone();
    sorted_ids.sort();
    assert_eq!(dels, sorted_ids);
    let adds: Vec<&Value> = extra.iter().filter(|op| op[0] == "add").collect();
    assert_eq!(adds.len(), 1);
    assert_eq!(
        (adds[0][1].as_str(), adds[0][2].as_str()),
        (Some(TRIM_ID), Some("main"))
    );

    let snap = e.snapshot();
    let root = &snap.surfaces[0].root;
    let main = root.c.iter().find(|n| n.id == "main").unwrap();
    let main_ids = child_ids(main);
    assert_eq!(main_ids.first(), Some(&TRIM_ID));
    assert_eq!(main_ids.last(), Some(&"r10"));
    assert!(!main_ids.contains(&"r1"));
    let dock = root.c.iter().find(|n| n.id == "dock").unwrap();
    assert_eq!(child_ids(dock), ["d1"]);
}

#[test]
fn a_late_frame_naming_evicted_ids_fails_per_op_with_one_error_and_one_ack() {
    // omp sent this frame before it read our `gone` (reference 3.5), so it may still name evicted ids.
    let (mut e, t, ids) = evicted();
    let gone = ids[0].as_str();
    let good =
        json!(["add", "late", "dock", null, {"id": "late", "k": "text", "p": {"text": "ok"}}]);
    let ops = json!([
        ["add", "x", "main", gone, {"id": "x", "k": "text", "p": {"text": "x"}}],
        ["set", gone, {"text": "y"}],
        ["del", gone],
        good
    ]);
    let steps = e.on_output(t, &frame_bytes(SF, 3, ops));

    let Step::Update(TspChangeFor {
        change: TspChange::Frame { seq, ops, .. },
        ..
    }) = &steps[0]
    else {
        panic!("{steps:?}")
    };
    assert_eq!(
        (*seq, ops.as_slice()),
        (3, std::slice::from_ref(&good)),
        "only the op that applied is forwarded"
    );

    let bodies = reply_bodies(&steps);
    let errors: Vec<&Value> = bodies.iter().filter(|b| b["ev"] == "error").collect();
    assert_eq!(
        errors.len(),
        1,
        "three failed ops, one error event: {bodies:?}"
    );
    let msg = errors[0]["msg"].as_str().unwrap();
    assert!(
        msg.starts_with("frame 3: ")
            && msg.contains("op 0")
            && msg.contains("op 1")
            && msg.contains("op 2"),
        "{msg}"
    );
    assert_eq!(
        bodies.last(),
        Some(&ack(SF, 3)),
        "the frame is acked normally"
    );
    assert_eq!(bodies.iter().filter(|b| b["ev"] == "ack").count(), 1);
    assert!(
        !bodies.iter().any(|b| b["ev"] == "gone"),
        "nothing new was evicted"
    );
}
