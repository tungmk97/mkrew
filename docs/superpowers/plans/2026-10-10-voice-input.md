# Voice Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user speak instead of typing, in Vietnamese, accented English or both mixed. A shortcut records the microphone. An OpenAI-compatible speech-to-text API (Groq by default) turns the recording into text. The text is typed into the text field or terminal pane that has the keyboard.

**Architecture:** Everything runs in the UI process (`mkrew-ui`). The daemon, protocol and database do not change.
- `voice.rs` records the default microphone on its own thread (cpal) and builds a 16-bit mono WAV.
- `stt.rs` POSTs the WAV as multipart data to `{base_url}/audio/transcriptions`. It uses ureq (blocking) and runs on the GPUI background executor.
- `Shell` starts and stops recording with `Command::VoiceInput` and shows the state in the status bar.
- The text goes to the focused field through that field's own IME path (`ime::keys` → the field's key handler). If a terminal is focused instead, it goes through `TerminalPane::type_text`. If neither has focus, it goes to the clipboard.
- Settings live in a new `[voice]` section of `settings.toml`. The API key comes from an environment variable and is never stored in the file.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), new crates `cpal 0.18.2` (microphone) and `ureq 3.4.2` (HTTPS, rustls), already present: `async-channel 2`, `serde_json 1`, `ulid 1`, `anyhow 1`.

**Spec:** The design agreed in chat on 2026-10-10 (below). Task 4 writes it into `docs/SPEC.md` as §20G and amends §1 (the non-goal "never calls LLM APIs"), §4, §13 and §20.0. It also adds ADR `docs/decisions/0045-voice-input.md`. Executors use this plan's Design section as the spec.

## Design (agreed in chat, 2026-10-10)

- **User need:** the user is Vietnamese and speaks intermediate English with an accent. Voice input must work in **every input of the app** (the user picked option 3): composer, task dialog, commit box, comments, peek reply, palette, filters, Files editor and terminal panes.
- **Engine:** a cloud API that speaks the OpenAI format. The default is Groq `whisper-large-v3-turbo`: about $0.04 per audio hour, has a free tier, and Whisper handles Vietnamese and accented English. To switch provider (OpenAI, a local OpenAI-compatible server), change only `base_url`, `model` and `api_key_env`. Local whisper.cpp is **not** in this plan; add it only if offline use is ever needed.
- **Key:** the user already has a Groq key. It goes in the environment variable `GROQ_API_KEY` (the name is configurable), never in `settings.toml`. A `[voice] api_key` key is a settings error.
- **Interaction:** a toggle shortcut, `Ctrl+Shift+D` ("dictate"). Press once to start, press again to stop. Hold-to-talk is not possible: `mkrew-ui` has no key-up handling (no `on_key_up` anywhere), and GPUI bindings fire on key down.
- **Language:** detected per recording by default (`language = ""`). The default `prompt` mixes Vietnamese and English coding words so that English terms stay in English.
- **Target:** the text goes to the field or terminal that has the keyboard **when the text arrives**. If nothing that takes text has focus, the text goes to the clipboard. It is never lost and never typed as bare-key Git shortcuts (`s`, `c`, …).

## Workspace (separate git worktree)

All tasks run in their own git worktree on branch `feat/voice-input`, never in the main checkout on `master`. The main checkout holds the user's uncommitted edits, among them `crates/mkrew-ui/src/main.rs`, which Task 4 also changes. A worktree starts from a clean `HEAD`, so a task's `git add <path>` can never pick up those edits.

Setup, once, before Task 1 (from the main checkout):

1. `.worktrees/` must be git-ignored. Today `git check-ignore -q .worktrees` fails, so append `/.worktrees/` to `.gitignore` and commit only that file: `git add .gitignore && git commit -m "chore: ignore .worktrees"`.
2. Check free space on C:. The worktree gets its own `target/`: a full debug build of the workspace plus cpal and ureq. On 2026-10-10 the main checkout's `target/debug` took 33 GB, though that includes old incremental files, and C: had 44 GB free. If less than 35 GB is free, stop and ask the user. Do not delete anything in the main checkout's `target/` to make room.
3. `git worktree add .worktrees/voice-input -b feat/voice-input`
4. Baseline in the worktree: `cargo test --workspace -j 4`. If it fails, report the failures and ask before Task 1.

Rules for every task:

- Run every command with the worktree (`.worktrees/voice-input`) as the working directory. All paths in this plan are relative to the worktree root. Superpowers scripts (`sh scripts/sp.sh …`) also run from there, so the SDD ledger lives in the worktree's `.superpowers/sdd/`.
- Commit on `feat/voice-input` only. Never push.
- When all tasks are done, use superpowers:finishing-a-development-branch. Merging into `master` needs the user's OK; after the merge, `git worktree remove .worktrees/voice-input` and `git branch -d feat/voice-input`.

## Global Constraints

- Every color, spacing and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14). The status-bar item uses only `notice()` and theme colors.
- No `unwrap`/`expect` outside tests. `mkrew-ui` is a binary crate, so use `anyhow` with context there; `thiserror` stays in libraries.
- `unsafe_code = "deny"` workspace-wide: use cpal's safe API only.
- No test may call a real agent or the network (SPEC §16). The one HTTP test talks only to a `TcpListener` on `127.0.0.1` that the test itself starts.
- Do not touch `packaging/`, `docs/BUILD_VI.md`, `scripts/package-setup.ps1`, or the `x11`/`wayland` line in `crates/mkrew-ui/Cargo.toml`.
- No protocol change (`PROTOCOL_VERSION` stays 37), no migration.
- ADR number **0045**: 0041–0044 are reserved by `docs/superpowers/plans/2026-10-10-inbox-perf.md`. Before writing the ADR, run `ls docs/decisions` and take the next free number if 0045 is taken.
- The C: drive is often nearly full. Build with `-j 4`, and check free space before the first build of a task that adds crates (Tasks 2 and 3).
- Shell: run `cargo` and `git` directly. Run Git Bash scripts with `sh`; in the omp shell, `bash` is WSL, so never use it.
- Commits: after each task, commit only that task's files (`git add <paths>`, never `-A` or `.`). Use Conventional Commits. Never push. If `git status` shows changes in `Cargo.lock` or `.github/workflows/` **before** Task 2 starts, stop and ask the user: they may be the user's own edits.
- GPUI gotchas: never call `.hover()` twice on one element. Never update the `Shell` entity from inside its own `update`; call the handler directly on `self` instead.
- Exhaustive `match`es on `Command`: if the compiler reports one this plan does not list, add `Command::VoiceInput` there with the same behavior as `Command::ToggleInspector`.

## Review Focus

1. **Silence or a slip of the shortcut sends audio anyway.** Whisper invents text for silence ("Cảm ơn các bạn đã xem video"), and that text would be typed into a prompt. Expected: recordings under 0.3 s, or with a peak under 0.01, are never sent, and the user sees "too short" or "heard nothing". Pinned by `voice::tests::only_speech_long_and_loud_enough_is_sent` (Task 2).
2. **Line breaks in the transcript reach an agent as Enter and send a half-finished prompt.** Expected: all whitespace collapses to single spaces, with no leading space. Pinned by `stt::tests::answers_become_text_or_the_apis_reason` (Task 3). `type_text` never adds Enter.
3. **A bad or missing key shows a bare "HTTP 401", or nothing at all.** Expected: a missing variable names the variable, and an API error shows the API's own `error.message`. Pinned by `stt::tests::api_key_comes_from_the_named_variable` and `answers_become_text_or_the_apis_reason` (Task 3).
4. **Wrong request shape** (boundary, auth header or field names), so the real provider rejects every request. Pinned by the loopback test `stt::tests::transcribe_posts_multipart_with_the_key_and_reads_the_text` (Task 3).
5. **Focus moved during transcription** (for example, the user clicked the Git list). The text must not be typed as bare-key shortcuts. Expected: it goes to the clipboard with a status message. The design prevents this: it never uses `window.dispatch_keystroke`, and it calls only the recorded field's handler after checking `is_focused`. GPUI cannot run headless, so this is a manual check in HUMAN_CHECKS (Task 4).

---

### Task 1: `[voice]` settings

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (struct `Settings` L16-25, new `VoiceSettings`, `template()` L269-303, tests `missing_file_is_defaults` L360 and `full_file_parses` L403, one new test)
- Modify: `docs/SPEC.md` §20.0 settings TOML block (ends L1541-1542)

**Interfaces:**
- Produces (used by Tasks 3 and 4):
  - `mkrew_core::settings::VoiceSettings { base_url: String, model: String, api_key_env: String, language: String, prompt: String, max_seconds: u32 }`, deriving `Debug, Clone, PartialEq, Eq, Deserialize`, with a hand-written `Default`
  - `Settings.voice: VoiceSettings`
  - `pub const VOICE_PROMPT_DEFAULT: &str`

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-core/src/settings.rs`, in test `missing_file_is_defaults`, add after `assert_eq!(settings.zed.follow_debounce_ms, 1500);`:

```rust
        assert_eq!(settings.voice.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(settings.voice.model, "whisper-large-v3-turbo");
        assert_eq!(settings.voice.api_key_env, "GROQ_API_KEY");
        assert_eq!(settings.voice.language, "");
        assert_eq!(settings.voice.prompt, VOICE_PROMPT_DEFAULT);
        assert_eq!(settings.voice.max_seconds, 120);
```

In test `full_file_parses`, in the expected `Settings { … }` literal, add after the `terminal: FontSettings { … },` field:

```rust
                voice: VoiceSettings::default(),
```

Add this test after `bad_type_is_an_error`:

```rust
    #[test]
    fn voice_section_parses_another_provider_and_rejects_a_key() {
        let voice = parse(
            Path::new("settings.toml"),
            "[voice]\nbase_url = \"https://api.openai.com/v1\"\nmodel = \"gpt-4o-mini-transcribe\"\n\
             api_key_env = \"OPENAI_API_KEY\"\nlanguage = \"vi\"\nmax_seconds = 30\n",
        )
        .unwrap()
        .voice;
        assert_eq!(
            voice,
            VoiceSettings {
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4o-mini-transcribe".into(),
                api_key_env: "OPENAI_API_KEY".into(),
                language: "vi".into(),
                prompt: VOICE_PROMPT_DEFAULT.into(),
                max_seconds: 30,
            }
        );
        // The key itself never lives in the file (SPEC §20G).
        let message = parse_err("[voice]\napi_key = \"gsk_x\"\n");
        assert!(message.contains("unknown field `api_key`"), "{message}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-core settings -j 4`
Expected: FAIL to compile, with `no field voice on type Settings`, `cannot find type VoiceSettings` and `cannot find value VOICE_PROMPT_DEFAULT`.

- [ ] **Step 3: Add the section**

In `struct Settings`, after `pub terminal: FontSettings,` add:

```rust
    /// `[voice]`: speech to text for voice input (SPEC §20G, ADR 0045).
    pub voice: VoiceSettings,
```

After `impl Default for ZedSettings { … }` (ends L89) add:

```rust
/// The default `[voice] prompt`: words Whisper should spell as written, in both languages, so
/// English coding terms in Vietnamese speech stay English.
pub const VOICE_PROMPT_DEFAULT: &str = "Mkrew, Claude Code, Codex, commit, branch, merge, refactor, test, bug. Sửa lỗi này rồi chạy test lại nhé.";

/// `[voice]` (SPEC §20G): an OpenAI-compatible speech-to-text endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceSettings {
    /// API root; the request goes to `{base_url}/audio/transcriptions`.
    pub base_url: String,
    pub model: String,
    /// Name of the environment variable holding the API key; empty = send no key (a local
    /// server). The key itself is never read from this file.
    pub api_key_env: String,
    /// ISO-639-1 code (`vi`, `en`); empty = the model detects it per recording.
    pub language: String,
    /// Spelling and style hint (Whisper `prompt`, 224 tokens at most); empty = none.
    pub prompt: String,
    /// A recording stops by itself after this many seconds.
    pub max_seconds: u32,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            base_url: "https://api.groq.com/openai/v1".into(),
            model: "whisper-large-v3-turbo".into(),
            api_key_env: "GROQ_API_KEY".into(),
            language: String::new(),
            prompt: VOICE_PROMPT_DEFAULT.into(),
            max_seconds: 120,
        }
    }
}
```

In `template()`, replace the last line `     # font_size = 13\n"` (L302) with:

```rust
     # font_size = 13\n\
     \n\
     # Voice input (Ctrl+Shift+D): speech to text over an OpenAI-compatible API, Groq by\n\
     # default (OpenAI and local servers work too). The API key is read from the environment\n\
     # variable named by api_key_env (empty: no key); restart Mkrew after setting it.\n\
     # language: empty detects it per recording; \"vi\" or \"en\" forces one.\n\
     # prompt: words the model should spell as written.\n\
     [voice]\n\
     # base_url = \"https://api.groq.com/openai/v1\"\n\
     # model = \"whisper-large-v3-turbo\"\n\
     # api_key_env = \"GROQ_API_KEY\"\n\
     # language = \"\"\n\
     # prompt = \"Mkrew, Claude Code, Codex, commit, branch, merge, refactor, test, bug. Sửa lỗi này rồi chạy test lại nhé.\"\n\
     # max_seconds = 120\n"
```

The prose comment lines must not contain ` = `, because `the_template_parses_to_the_defaults_commented_or_not` uncomments every `# key = value` line.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-core settings -j 4`
Expected: PASS, including `the_template_parses_to_the_defaults_commented_or_not` (the template values equal the defaults).

- [ ] **Step 5: SPEC §20.0**

In `docs/SPEC.md` §20.0, inside the ```` ```toml ```` block, insert the following after the line `font_size = 13              # px, 8–32; also the Files and Git code views` and before the closing fence:

```toml

[voice]                     # §20G, ADR 0045
base_url = "https://api.groq.com/openai/v1"  # OpenAI-compatible; POST {base_url}/audio/transcriptions
model = "whisper-large-v3-turbo"
api_key_env = "GROQ_API_KEY"  # env var holding the key ("" = no key, local server); never the key itself
language = ""               # "" = detected per recording; "vi" | "en" forces one
prompt = "Mkrew, Claude Code, Codex, commit, branch, merge, refactor, test, bug. Sửa lỗi này rồi chạy test lại nhé."
max_seconds = 120           # a recording stops by itself after this
```

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-core --all-targets -j 4 -- -D warnings
git add crates/mkrew-core/src/settings.rs docs/SPEC.md
git commit -m "feat(core): [voice] settings for speech-to-text input"
```

---

### Task 2: Microphone capture (`voice.rs`)

**Files:**
- Create: `crates/mkrew-ui/src/voice.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (module list and doc comment)
- Modify: `crates/mkrew-ui/Cargo.toml` (add `cpal`)
- Modify: `Cargo.lock` (cargo updates it)
- Modify: `.github/workflows/ci.yml` L41 and `.github/workflows/release.yml` L33 (Linux needs `libasound2-dev` to build cpal's ALSA backend)

**Interfaces:**
- Produces (used by Task 4):
  - `pub enum voice::State { Idle, Recording { since: Instant, stop: Stop }, Transcribing, Message(String) }`, whose `Default` is `Idle`
  - `pub struct voice::Stop`: dropping it ends the recording
  - `pub struct voice::Clip { pub samples: Vec<f32>, pub rate: u32 }` and `Clip::into_wav(self) -> anyhow::Result<Vec<u8>>`
  - `pub fn voice::record(max: Duration) -> (Stop, async_channel::Receiver<anyhow::Result<Clip>>)`
  - `pub fn voice::wav(samples: &[f32], rate: u32) -> Vec<u8>`
  - `pub fn voice::push_mono<T: Copy>(data: &[T], channels: usize, out: &mut Vec<f32>) where f32: cpal::FromSample<T>`

- [ ] **Step 1: Add the dependency and the CI package**

Check free space on C: first. In `crates/mkrew-ui/Cargo.toml`, add after the `async-channel = "2"` line:

```toml
# Microphone capture for voice input (SPEC §20G, ADR 0045): WASAPI on Windows, ALSA on Linux.
cpal = "0.18.2"
```

In `.github/workflows/ci.yml` L41 and `.github/workflows/release.yml` L33, append ` libasound2-dev` to the `apt-get install -y …` list, on the same line after `libfontconfig-dev`.

- [ ] **Step 2: Write the module with its tests**

Create `crates/mkrew-ui/src/voice.rs`:

```rust
//! Voice input capture (SPEC §20G): the default microphone on its own thread, mixed to mono,
//! and the 16-bit WAV sent for speech to text.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, bail};
use cpal::FromSample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Shorter recordings are a slip of the shortcut, not speech.
pub const MIN_SECONDS: f32 = 0.3;
/// A peak below this (about -40 dBFS) is silence; Whisper invents text for silence.
pub const MIN_PEAK: f32 = 0.01;

/// The shell's voice input.
#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    /// Leaving this state drops `stop`, which ends the recording.
    Recording { since: Instant, stop: Stop },
    Transcribing,
    /// The last outcome worth showing (an error, or where the text went); the next start
    /// clears it.
    Message(String),
}

/// Ends the recording when dropped (the capture thread waits on the other end).
pub struct Stop {
    _alive: mpsc::Sender<()>,
}

/// A finished recording: mono samples in -1..=1 at the microphone's rate.
pub struct Clip {
    pub samples: Vec<f32>,
    pub rate: u32,
}

impl Clip {
    /// The WAV to send, unless the clip is too short or silent to be speech.
    pub fn into_wav(self) -> Result<Vec<u8>> {
        let seconds = self.samples.len() as f32 / self.rate.max(1) as f32;
        if seconds < MIN_SECONDS {
            bail!("too short: press the shortcut, speak, then press it again");
        }
        let peak = self.samples.iter().fold(0f32, |peak, s| peak.max(s.abs()));
        if peak < MIN_PEAK {
            bail!("heard nothing: is the microphone muted?");
        }
        Ok(wav(&self.samples, self.rate))
    }
}

/// Starts recording the default microphone until `Stop` is dropped or `max` passes. The clip
/// (or why there is none) arrives once on the returned channel.
pub fn record(max: Duration) -> (Stop, async_channel::Receiver<Result<Clip>>) {
    let (alive, stopped) = mpsc::channel::<()>();
    let (done, clip) = async_channel::bounded(1);
    let failed = done.clone();
    let spawned = std::thread::Builder::new()
        .name("mkrew-voice".into())
        .spawn(move || {
            let _ = done.send_blocking(capture(&stopped, max));
        });
    if let Err(error) = spawned {
        let _ = failed.try_send(Err(anyhow!(error).context("cannot start the microphone thread")));
    }
    (Stop { _alive: alive }, clip)
}

fn capture(stopped: &mpsc::Receiver<()>, max: Duration) -> Result<Clip> {
    let device = cpal::default_host()
        .default_input_device()
        .context("no microphone found")?;
    let supported = device
        .default_input_config()
        .context("cannot read the microphone format")?;
    let channels = usize::from(supported.channels());
    let rate = supported.sample_rate();
    let config = supported.config();
    let samples = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => input::<f32>(&device, config, channels, samples.clone()),
        cpal::SampleFormat::I16 => input::<i16>(&device, config, channels, samples.clone()),
        other => bail!("unsupported microphone format {other:?}"),
    }
    .context("cannot open the microphone")?;
    stream.play().context("cannot start the microphone")?;
    // Nothing is ever sent: this returns when the shell drops its `Stop` or after `max`.
    let _ = stopped.recv_timeout(max);
    drop(stream);
    let samples = std::mem::take(
        &mut *samples
            .lock()
            .map_err(|_| anyhow!("the microphone buffer broke"))?,
    );
    Ok(Clip { samples, rate })
}

fn input<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    sink: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample,
    f32: FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if let Ok(mut out) = sink.lock() {
                push_mono(data, channels, &mut out);
            }
        },
        |error| tracing::warn!("microphone: {error}"),
        None,
    )
}

/// Appends interleaved `data` of `channels` channels as mono (the mean of each frame).
pub fn push_mono<T: Copy>(data: &[T], channels: usize, out: &mut Vec<f32>)
where
    f32: FromSample<T>,
{
    for frame in data.chunks(channels.max(1)) {
        let sum: f32 = frame.iter().map(|&s| f32::from_sample_(s)).sum();
        out.push(sum / frame.len() as f32);
    }
}

/// 16-bit PCM mono WAV of `samples` (clamped to -1..=1) at `rate` Hz.
// ponytail: the mic's own rate (48 kHz mono ≈ 96 KB/s, 120 s ≈ 11.5 MB, under Groq's 25 MB);
// resample to 16 kHz if uploads get slow.
pub fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&data.saturating_add(36).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&rate.saturating_mul(2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        // The clamp keeps the cast in range.
        let pcm = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&pcm.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_mono_averages_each_frame() {
        let mut out = Vec::new();
        push_mono(&[0.5f32, -0.5, 1.0, 0.0], 2, &mut out);
        assert_eq!(out, [0.0, 0.5]);
        push_mono(&[i16::MAX, i16::MIN], 1, &mut out);
        assert_eq!(out.len(), 4);
        assert!((out[2] - 1.0).abs() < 1e-3, "{out:?}");
        assert!((out[3] + 1.0).abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn wav_has_a_pcm_header_and_clamped_samples() {
        let bytes = wav(&[0.0, 1.0, -2.0], 16_000);
        let u32_at =
            |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        assert_eq!(bytes.len(), 44 + 6);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32_at(4), 36 + 6);
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16_at(20), 1, "PCM");
        assert_eq!(u16_at(22), 1, "mono");
        assert_eq!(u32_at(24), 16_000);
        assert_eq!(u32_at(28), 32_000);
        assert_eq!(u16_at(34), 16);
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32_at(40), 6);
        let pcm: Vec<i16> = bytes[44..]
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(pcm, [0, i16::MAX, -i16::MAX]);
    }

    #[test]
    fn only_speech_long_and_loud_enough_is_sent() {
        let clip = |samples: usize, level: f32| Clip {
            samples: vec![level; samples],
            rate: 1000,
        };
        let short = clip(200, 0.5).into_wav().unwrap_err().to_string();
        assert!(short.contains("too short"), "{short}");
        let silent = clip(2000, 0.005).into_wav().unwrap_err().to_string();
        assert!(silent.contains("heard nothing"), "{silent}");
        // The peak is by magnitude: a negative-only signal is speech too.
        assert_eq!(clip(2000, -0.5).into_wav().unwrap().len(), 44 + 4000);
        // Exactly the minimum length and just above the minimum level are sent.
        assert_eq!(clip(300, 0.02).into_wav().unwrap().len(), 44 + 600);
    }
}
```

In `crates/mkrew-ui/src/lib.rs`, add `pub mod voice;` after `pub mod usage;`. In the module doc comment, change `the Run button, and` to `the Run button, voice input (microphone and speech to text), and`.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p mkrew-ui --lib voice -j 4`
Expected: PASS (3 tests). The first build of cpal and its Windows dependencies takes a few minutes. If the compiler rejects `f32::from_sample_`, look up `cpal::FromSample` on docs.rs (it is re-exported from `dasp_sample`) and use the exact method name shown there. Do not write the conversion by hand.

- [ ] **Step 4: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
git add crates/mkrew-ui/src/voice.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/Cargo.toml Cargo.lock .github/workflows/ci.yml .github/workflows/release.yml
git commit -m "feat(ui): microphone capture for voice input"
```

The `cpal`/`record` path needs a real microphone, so Task 4's HUMAN_CHECKS cover it by hand.

---

### Task 3: Speech-to-text client (`stt.rs`)

**Files:**
- Create: `crates/mkrew-ui/src/stt.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (module list)
- Modify: `crates/mkrew-ui/Cargo.toml` (add `ureq`)
- Modify: `Cargo.lock` (cargo updates it)

**Interfaces:**
- Consumes from Task 1: `mkrew_core::settings::VoiceSettings`
- Produces (used by Task 4):
  - `pub fn stt::api_key(settings: &VoiceSettings, var: impl Fn(&str) -> Option<String>) -> anyhow::Result<Option<String>>`
  - `pub fn stt::transcribe(settings: &VoiceSettings, api_key: Option<&str>, wav: &[u8]) -> anyhow::Result<String>`, which returns the cleaned text (possibly empty)
  - helpers `url`, `body`, `parse` and `clean`, all `pub` for the tests

- [ ] **Step 1: Add the dependency**

In `crates/mkrew-ui/Cargo.toml`, add after the `ulid = …` line:

```toml
# HTTPS POST of voice recordings to the speech-to-text API (SPEC §20G, ADR 0045); blocking
# (runs on the background executor), rustls so no OpenSSL on any OS.
ureq = { version = "3.4.2", default-features = false, features = ["rustls"] }
```

- [ ] **Step 2: Write the module with its tests**

Create `crates/mkrew-ui/src/stt.rs`:

```rust
//! Speech to text for voice input (SPEC §20G): one `POST {base_url}/audio/transcriptions`
//! (OpenAI-compatible: Groq, OpenAI, local servers) with the WAV as multipart form data.

use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use mkrew_core::settings::VoiceSettings;

/// The whole call (upload and model) gives up after this.
const TIMEOUT: Duration = Duration::from_secs(60);

/// The API key from the environment variable `api_key_env` names (`var` reads it); `None` when
/// the name is empty (a local server without keys).
pub fn api_key(
    settings: &VoiceSettings,
    var: impl Fn(&str) -> Option<String>,
) -> Result<Option<String>> {
    let name = settings.api_key_env.trim();
    if name.is_empty() {
        return Ok(None);
    }
    match var(name).map(|key| key.trim().to_owned()) {
        Some(key) if !key.is_empty() => Ok(Some(key)),
        _ => bail!("set the {name} environment variable to your API key, then restart Mkrew"),
    }
}

pub fn url(base_url: &str) -> String {
    format!("{}/audio/transcriptions", base_url.trim_end_matches('/'))
}

/// The multipart form: model, json answer, temperature 0, language and prompt when set, then
/// the WAV as `file`.
pub fn body(settings: &VoiceSettings, wav: &[u8], boundary: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(wav.len() + 1024);
    let mut field = |name: &str, value: &str| {
        if !value.is_empty() {
            out.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
                )
                .as_bytes(),
            );
        }
    };
    field("model", settings.model.trim());
    field("response_format", "json");
    field("temperature", "0");
    field("language", settings.language.trim());
    field("prompt", settings.prompt.trim());
    out.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\n\
             Content-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(wav);
    out.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    out
}

/// The text of a 2xx answer, or the API's reason for any other status.
pub fn parse(status: u16, body: &str) -> Result<String> {
    let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
    if (200..300).contains(&status) {
        let text = json
            .as_ref()
            .and_then(|json| json.get("text"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("unexpected answer: {}", short(body)))?;
        return Ok(clean(text));
    }
    let message = json
        .as_ref()
        .and_then(|json| json.pointer("/error/message"))
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| short(body), str::to_owned);
    bail!("HTTP {status}: {message}")
}

/// The first 200 chars of `body`, trimmed.
fn short(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(200) {
        Some((end, _)) => format!("{}…", &body[..end]),
        None => body.to_owned(),
    }
}

/// Whisper's text as one line with single spaces: a line break typed into an agent would be
/// Enter (send).
pub fn clean(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Sends `wav` for speech to text. Blocking: run it off the UI thread.
pub fn transcribe(settings: &VoiceSettings, api_key: Option<&str>, wav: &[u8]) -> Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        // 4xx / 5xx bodies carry the reason (bad key, rate limit): read them.
        .http_status_as_error(false)
        .build()
        .into();
    let boundary = format!("mkrew-{}", ulid::Ulid::new());
    let url = url(&settings.base_url);
    let mut request = agent.post(url.as_str()).header(
        "Content-Type",
        format!("multipart/form-data; boundary={boundary}"),
    );
    if let Some(key) = api_key {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    let mut response = request
        .send(&body(settings, wav, &boundary)[..])
        .with_context(|| format!("cannot reach {url}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .context("cannot read the answer")?;
    parse(status, &text)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};

    use super::*;

    #[test]
    fn api_key_comes_from_the_named_variable() {
        let settings = VoiceSettings::default();
        let key = api_key(&settings, |name| {
            (name == "GROQ_API_KEY").then(|| " gsk_1 \n".to_owned())
        });
        assert_eq!(key.unwrap(), Some("gsk_1".to_owned()));
        let missing = api_key(&settings, |_| None).unwrap_err().to_string();
        assert!(missing.contains("GROQ_API_KEY"), "{missing}");
        let blank = api_key(&settings, |_| Some("  ".into()))
            .unwrap_err()
            .to_string();
        assert!(blank.contains("GROQ_API_KEY"), "{blank}");
        let local = VoiceSettings {
            api_key_env: String::new(),
            ..VoiceSettings::default()
        };
        assert_eq!(api_key(&local, |_| panic!("no variable is read")).unwrap(), None);
    }

    #[test]
    fn url_joins_with_or_without_a_trailing_slash() {
        assert_eq!(
            url("https://api.groq.com/openai/v1"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        assert_eq!(
            url("http://127.0.0.1:8000/v1/"),
            "http://127.0.0.1:8000/v1/audio/transcriptions"
        );
    }

    #[test]
    fn body_has_the_set_fields_then_the_wav() {
        let settings = VoiceSettings {
            prompt: "Mkrew".into(),
            ..VoiceSettings::default()
        };
        let text = String::from_utf8(body(&settings, b"WAVDATA", "B")).unwrap();
        assert_eq!(
            text,
            "--B\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n\
             --B\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\njson\r\n\
             --B\r\nContent-Disposition: form-data; name=\"temperature\"\r\n\r\n0\r\n\
             --B\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nMkrew\r\n\
             --B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\n\
             Content-Type: audio/wav\r\n\r\nWAVDATA\r\n--B--\r\n"
        );
        let forced = VoiceSettings {
            language: "vi".into(),
            ..VoiceSettings::default()
        };
        let text = String::from_utf8(body(&forced, b"", "B")).unwrap();
        assert!(text.contains("name=\"language\"\r\n\r\nvi\r\n"), "{text}");
    }

    #[test]
    fn answers_become_text_or_the_apis_reason() {
        assert_eq!(
            parse(200, r#"{"text":" Xin chào,\nrun the tests. "}"#).unwrap(),
            "Xin chào, run the tests."
        );
        assert_eq!(parse(200, r#"{"text":""}"#).unwrap(), "");
        let bad_key = parse(
            401,
            r#"{"error":{"message":"Invalid API Key","type":"invalid_request_error"}}"#,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(bad_key, "HTTP 401: Invalid API Key");
        let gateway = parse(502, " <html>Bad Gateway</html>\n")
            .unwrap_err()
            .to_string();
        assert_eq!(gateway, "HTTP 502: <html>Bad Gateway</html>");
        let odd = parse(200, "{}").unwrap_err().to_string();
        assert!(odd.starts_with("unexpected answer"), "{odd}");
        let long = parse(500, &"x".repeat(500)).unwrap_err().to_string();
        assert_eq!(long, format!("HTTP 500: {}…", "x".repeat(200)));
    }

    /// Loopback only (`127.0.0.1`, SPEC §16): the exact request a real provider receives.
    #[test]
    fn transcribe_posts_multipart_with_the_key_and_reads_the_text() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let length: usize = head
                .lines()
                .find_map(|line| {
                    let line = line.to_ascii_lowercase();
                    let value = line.strip_prefix("content-length:")?.trim().to_owned();
                    value.parse().ok()
                })
                .unwrap();
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let answer = r#"{"text":" Sửa lỗi này, then run the tests."}"#;
            write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{answer}",
                answer.len()
            )
            .unwrap();
            (head, body)
        });
        let settings = VoiceSettings {
            base_url: format!("http://127.0.0.1:{port}/v1/"),
            ..VoiceSettings::default()
        };
        let text = transcribe(&settings, Some("gsk_test"), b"RIFFfake").unwrap();
        assert_eq!(text, "Sửa lỗi này, then run the tests.");
        let (head, body) = server.join().unwrap();
        let head = head.to_ascii_lowercase();
        assert!(
            head.starts_with("post /v1/audio/transcriptions http/1.1\r\n"),
            "{head}"
        );
        assert!(head.contains("authorization: bearer gsk_test\r\n"), "{head}");
        assert!(
            head.contains("content-type: multipart/form-data; boundary=mkrew-"),
            "{head}"
        );
        let body = String::from_utf8_lossy(&body);
        assert!(
            body.contains("name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n"),
            "{body}"
        );
        assert!(
            body.contains("filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFfake\r\n"),
            "{body}"
        );
    }
}
```

In `crates/mkrew-ui/src/lib.rs`, add `pub mod stt;` after `pub mod status;`.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p mkrew-ui --lib stt -j 4`
Expected: PASS (5 tests). The loopback test fails or hangs if ureq sends `transfer-encoding: chunked` with no `content-length`. That means the body is not being sent as a sized `&[u8]`: fix the `send` call (keep `&body(..)[..]`), not the test.

- [ ] **Step 4: Lint and commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
git add crates/mkrew-ui/src/stt.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/Cargo.toml Cargo.lock
git commit -m "feat(ui): speech-to-text client for voice input"
```

---

### Task 4: Voice input in the shell (shortcut, typing, status bar, docs)

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum `Command` L20-129, `all()` L229-231, `name()` L301-303, `default_keys()` L379-381, test `spec_defaults` after L778)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title` L231-232, `available` L249-250, test `availability_follows_the_view` L840-852)
- Modify: `crates/mkrew-ui/src/ime.rs` (new `FocusedField` alias after `OnKey`, L70)
- Modify: `crates/mkrew-ui/src/main.rs`:
  - imports L64-80
  - `Shell` fields near L373 and their init near L774
  - status tick `due` L575
  - `Shell::typing` L4558-4565
  - `Shell::run` L7233 and the dispatcher arm near L7482
  - status bar L9019-9054
  - `typing_input` L17722-17744
  - `CardContext` L19306-19316, its init L10736, and the follow-up call L20108-20113
- Create: `docs/decisions/0045-voice-input.md`
- Modify: `docs/SPEC.md` (§1 L70, §4 table L153-175, §13 table after L1095, new §20G after §20F)
- Modify: `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`

**Interfaces:**
- Consumes: `voice::{State, record, Clip::into_wav}` (Task 2), `stt::{api_key, transcribe}` (Task 3), `VoiceSettings` (Task 1)
- Produces: `Command::VoiceInput` (keymap name `voice_input`, default `ctrl-shift-d`, context `None`) and `ime::FocusedField<V>`

- [ ] **Step 1: Write the failing tests (keymap and palette)**

In `crates/mkrew-ui/src/keymap.rs`, test `spec_defaults`, add after `assert_eq!(Command::ToggleInspector.context(), None);`:

```rust
        assert_eq!(bound("ctrl-shift-d"), Some(Command::VoiceInput));
        assert_eq!(keymap.label(Command::VoiceInput), "Ctrl+Shift+D");
        // Global: it must reach the shell while a terminal or a text field has the keyboard.
        assert_eq!(Command::VoiceInput.context(), None);
```

In `crates/mkrew-ui/src/palette.rs`, test `availability_follows_the_view`, in the expected list for `commands(&none)`, change `Command::ToggleCompactSidebar` to:

```rust
                Command::ToggleCompactSidebar,
                Command::VoiceInput
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib -j 4 -- keymap palette`
Expected: FAIL to compile with `no variant named VoiceInput`.

- [ ] **Step 3: Add the command**

In `keymap.rs`:
- In `enum Command`, after `ToggleInspector,` add:

  ```rust
      /// Start recording the microphone, or stop and type the speech as text into the field or
      /// terminal that has the keyboard (SPEC §20G).
      VoiceInput,
  ```

- In `all()`, after `Command::ToggleInspector,` add `Command::VoiceInput,`.
- In `name()`, after the `ToggleInspector` arm add `Command::VoiceInput => "voice_input".into(),`.
- In `default_keys()`, after the `ToggleInspector` arm add `Command::VoiceInput => "ctrl-shift-d".into(),`.

In `palette.rs`:
- In `title`, after the `ToggleInspector` arm add `Command::VoiceInput => "Global: Voice input (start / stop)".into(),`.
- In `available`, change `| Command::ToggleCompactSidebar => true,` to:

  ```rust
          | Command::ToggleCompactSidebar
          | Command::VoiceInput => true,
  ```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib -j 4 -- keymap palette`
Expected: the lib tests PASS. The `mkrew` binary does not compile yet because the match in `Shell::run` is missing `VoiceInput`; Step 6 fixes that.

- [ ] **Step 5: Record which field has the keyboard (`ime.rs`, `typing_input`)**

In `crates/mkrew-ui/src/ime.rs`, add after `pub type OnKey<V> = …;`:

```rust
/// The field whose input handler was registered on the last paint while focused, with its key
/// handler: voice text (SPEC §20G) is typed there the same way an IME commit is.
pub type FocusedField<V> = Rc<RefCell<Option<(FocusHandle, OnKey<V>)>>>;
```

In `crates/mkrew-ui/src/main.rs`:

1. In the `Shell` struct, after the `ime_marked` field (L373), add:

```rust
    /// The text field that had the keyboard at the last paint (voice input types there, §20G).
    typing_target: ime::FocusedField<Shell>,
    /// Voice input (§20G).
    voice: voice::State,
```

2. In `Shell::new`, after `ime_marked: Rc::default(),` (L774), add:

```rust
            typing_target: Rc::default(),
            voice: voice::State::default(),
```

3. Replace `typing_input` (L17722-17744) with:

```rust
fn typing_input(
    focus: &FocusHandle,
    on_key: ime::OnKey<Shell>,
    marked: &Rc<ime::Composing<FocusHandle>>,
    typing_target: &ime::FocusedField<Shell>,
    cx: &Context<'_, Shell>,
) -> impl IntoElement + use<> {
    let (focus, view, marked, typing_target) = (
        focus.clone(),
        cx.entity(),
        marked.clone(),
        typing_target.clone(),
    );
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            // Voice input types into the field that has the keyboard (§20G).
            if focus.is_focused(window) {
                typing_target.replace(Some((focus.clone(), on_key)));
            }
            let typing = ime::Typing {
                view,
                on_key,
                marked,
                focus: focus.clone(),
                caret: bounds,
            };
            window.handle_input(&focus, typing, cx);
        },
    )
    .absolute()
    .size_full()
}
```

4. In `Shell::typing` (L4564), change the body to `typing_input(focus, on_key, &self.ime_marked, &self.typing_target, cx)`.

5. In `struct CardContext`, after the `ime_marked` field (L19314), add:

```rust
    /// See [`Shell::typing`].
    typing_target: &'a ime::FocusedField<Shell>,
```

In its init, after `ime_marked: &self.ime_marked,` (L10736), add `typing_target: &self.typing_target,`. In the follow-up call (L20108-20113), add `cards.typing_target,` after `cards.ime_marked,`.

- [ ] **Step 6: Toggle, transcribe, type**

1. Imports: add `use mkrew_ui::stt;` and `use mkrew_ui::voice;` among the `use mkrew_ui::…` lines, keeping their alphabetical order (`stt` after `status`, `voice` after `usage`).

2. At the very top of `fn run` (L7233), before `// A shortcut ends every open popover`, add:

```rust
        // Voice input types into the field that has the keyboard, so it acts before the
        // focused-field checks below hand the key to that field, and before anything moves
        // the focus (§20G).
        if *command == Command::VoiceInput {
            return self.toggle_voice(window, cx);
        }
```

In the dispatcher match, after `Command::ToggleInspector => self.toggle_inspector(window, cx),`, add:

```rust
            // Handled at the top of `run`.
            Command::VoiceInput => {}
```

3. Add these methods to `impl Shell` right after the end of `fn run`:

```rust
    /// Voice input (§20G): start recording, or stop it; the text arrives in `voice_text`.
    fn toggle_voice(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match self.voice {
            // Leaving `Recording` drops its `Stop`: the clip comes to the task started below.
            voice::State::Recording { .. } => {
                self.voice = voice::State::Transcribing;
                cx.notify();
                return;
            }
            voice::State::Transcribing => return,
            voice::State::Idle | voice::State::Message(_) => {}
        }
        // Read per start: a hand edit of `[voice]` applies to the next recording.
        let settings = mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .voice;
        let key = match stt::api_key(&settings, |name| std::env::var(name).ok()) {
            Ok(key) => key,
            Err(error) => {
                self.voice = voice::State::Message(format!("✕ Voice: {error:#}"));
                cx.notify();
                return;
            }
        };
        let (stop, clip) = voice::record(Duration::from_secs(u64::from(settings.max_seconds)));
        self.voice = voice::State::Recording {
            since: Instant::now(),
            stop,
        };
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let wav = clip
                .recv()
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("the microphone thread stopped")))
                .and_then(voice::Clip::into_wav);
            let wav = match wav {
                Ok(wav) => wav,
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.voice = voice::State::Message(format!("✕ Voice: {error:#}"));
                        cx.notify();
                    });
                    return;
                }
            };
            // Also reached when `max_seconds` ended the recording.
            let _ = this.update(cx, |shell, cx| {
                shell.voice = voice::State::Transcribing;
                cx.notify();
            });
            let text = cx
                .background_executor()
                .spawn(async move { stt::transcribe(&settings, key.as_deref(), &wav) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| shell.voice_text(text, window, cx));
        })
        .detach();
    }

    /// Where a transcript goes: the field or terminal with the keyboard, else the clipboard.
    fn voice_text(
        &mut self,
        text: anyhow::Result<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.voice = match text {
            Err(error) => voice::State::Message(format!("✕ Voice: {error:#}")),
            Ok(text) if text.is_empty() => voice::State::Message("Voice: heard nothing".into()),
            Ok(text) => {
                if self.type_voice_text(&text, window, cx) {
                    voice::State::Idle
                } else {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    voice::State::Message(
                        "Voice: no text field has the keyboard; the text is on the clipboard"
                            .into(),
                    )
                }
            }
        };
        cx.notify();
    }

    /// Types `text` into the text field that has the keyboard (like an IME commit) or into the
    /// focused terminal pane (one paste, never Enter). False when neither has focus: nothing is
    /// typed, so bare-key shortcuts (Git list) never fire.
    fn type_voice_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let field = self
            .typing_target
            .borrow()
            .clone()
            .filter(|(focus, _)| focus.is_focused(window));
        if let Some((_, on_key)) = field {
            for keystroke in ime::keys(&Cell::new(0), text, false) {
                let event = KeyDownEvent {
                    keystroke,
                    is_held: false,
                    prefer_character_input: false,
                };
                on_key(self, &event, window, cx);
            }
            return true;
        }
        let pane = self
            .panes
            .values()
            .find(|pane| pane.read(cx).has_focus(window, cx))
            .cloned();
        if let Some(pane) = pane {
            pane.update(cx, |pane, cx| pane.type_text(text, cx));
            return true;
        }
        false
    }
```

4. Status tick (L575): change `let due = board_due || shell.starting_pane.is_some();` to:

```rust
                    let recording = matches!(shell.voice, voice::State::Recording { .. });
                    let due = board_due || shell.starting_pane.is_some() || recording;
```

5. Status bar (`render_status_bar`): after the `let git = …;` statement (ends L9029), add:

```rust
        // Voice input (§20G): recording time, transcribing, or the last outcome.
        let voice_status = match &self.voice {
            voice::State::Idle => None,
            voice::State::Recording { since, .. } => {
                let seconds = since.elapsed().as_secs();
                Some(notice(
                    theme.status_needs_input,
                    format!(
                        "● {}:{:02} · {} stops",
                        seconds / 60,
                        seconds % 60,
                        self.keymap.label(Command::VoiceInput)
                    ),
                ))
            }
            voice::State::Transcribing => {
                Some(notice(theme.text_muted, "◌ Transcribing…".to_owned()))
            }
            voice::State::Message(text) => Some(notice(theme.text_muted, text.clone())),
        };
```

Then change `.children(git)` (L9054) to `.children(git).children(voice_status)`.

- [ ] **Step 7: Build, lint, test**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui --lib -j 4 -- keymap palette ime voice stt
```

Expected: clippy is clean and all tests PASS.

- [ ] **Step 8: ADR**

Run `ls docs/decisions`. If 0045 is free, create `docs/decisions/0045-voice-input.md`:

```markdown
# 0045 — Voice input through an OpenAI-compatible speech-to-text API

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; user request "nói thay vì gõ, tiếng Việt và tiếng Anh")

## Context

- The user is Vietnamese and speaks intermediate English with an accent. They want to speak
  instead of typing in every input of the app (prompts, dialogs, commit box, terminals).
- SPEC §1 said Mkrew never calls LLM APIs itself. Speech to text needs a model, either in the
  process or hosted. In the process means whisper.cpp: about 1.6 GB of model, a C++ build, and
  seconds per sentence on the user's laptop, which has no GPU.
- Every text field already takes IME commits through `ime::Typing`, which feeds the field's own
  key handler. Terminals take pastes through `TerminalPane::type_text`.

## Options

1. Windows voice typing (Win+H). No code, but Windows only and outside our control.
2. Local whisper.cpp through `whisper-rs`. Offline and free, but it adds a C++ toolchain to the
   build, a large model download, and slow transcription without a GPU.
3. An OpenAI-compatible `POST /audio/transcriptions` endpoint. Groq `whisper-large-v3-turbo` is
   the default; OpenAI and local OpenAI-compatible servers work by changing `[voice]`. (Chosen.)

## Decision

- `Voice input` (Ctrl+Shift+D, palette) starts recording the default microphone (cpal). The same
  key stops it, and `[voice] max_seconds` stops it by itself. It is a toggle, not hold-to-talk,
  because the UI has no key-up handling.
- The clip is sent as 16-bit mono WAV in multipart form data (ureq, rustls) to
  `{base_url}/audio/transcriptions`. The key comes from the environment variable that
  `api_key_env` names; it is never stored in `settings.toml`.
- Recordings under 0.3 s, or with a peak under 0.01, are not sent: Whisper invents text for silence.
- The text, with whitespace collapsed to single spaces, is typed like an IME commit into the text
  field that has the keyboard when it arrives, or pasted into the focused terminal pane. If
  neither has focus, it goes to the clipboard.

## Consequences

- SPEC §1 (amended non-goal), §4, §13, §20.0 and the new §20G change in the same commit.
- Audio leaves the machine only when the user starts a recording. Nothing is stored.
- Groq's free tier has rate limits. Errors show the API's message in the status bar.
- Linux builds need `libasound2-dev`, which CI now installs. On macOS, voice input needs
  `NSMicrophoneUsageDescription` in the app bundle's Info.plist; packaging is not changed here.
- On Windows, "Let desktop apps access your microphone" must be on (Settings → Privacy).
- Voice text reaches a field one char at a time, like an IME commit, so the Files editor gets
  one undo step per char.
```

If 0045 is taken, use the next free number here, in SPEC and in the code comments.

- [ ] **Step 9: SPEC**

In `docs/SPEC.md`:

1. In §1, replace the line `- Mkrew never calls LLM APIs itself; it only runs CLI agents the user installed.` with:

```markdown
- Mkrew never calls LLM APIs itself; it only runs CLI agents the user installed. One exception:
  voice input (§20G) sends a recording the user started to the speech-to-text API they configured
  (ADR 0045).
```

2. In the §4 table, after the `| Diff | \`similar\` | |` row, add:

```markdown
| Microphone | `cpal` | voice input (§20G); WASAPI on Windows, ALSA on Linux |
| HTTPS client | `ureq` (rustls) | voice input speech to text (§20G) |
```

3. In the §13 table, after `| Global | Toggle compact sidebar | Ctrl+Alt+B |`, add:

```markdown
| Global | Voice input: start / stop recording (§20G) | Ctrl+Shift+D |
```

4. After the last line of §20F (`  through fixture transcripts.`), add:

```markdown

### §20G Voice input (outside the milestones, ADR 0045)

Speak instead of typing, in Vietnamese or English (mixed and accented speech included).

1. **Start / stop.** `Voice input` (Ctrl+Shift+D, palette "Global: Voice input (start / stop)")
   records the default microphone, and the same key stops it. It works everywhere, also while a
   terminal, a text field or a dialog has the keyboard. A recording stops by itself after
   `[voice] max_seconds` (§20.0). While transcribing, the key does nothing.
2. **Status bar.** `● m:ss · Ctrl+Shift+D stops` while recording, `◌ Transcribing…` while
   waiting, nothing after success. Errors and notes (`✕ Voice: …`) stay until the next start.
3. **Speech to text.** One `POST {base_url}/audio/transcriptions` (OpenAI-compatible: Groq by
   default, OpenAI, or a local server) carries the recording as 16-bit mono WAV in multipart form
   data, with `model`, `response_format=json`, `temperature=0`, and `language` / `prompt` when set.
   The timeout is 60 s. The key is read from the environment variable that `api_key_env` names
   (never from `settings.toml`) and sent as `Authorization: Bearer`; an empty `api_key_env` sends
   no key. Recordings under 0.3 s, or with a peak under 1 % of full scale, are not sent ("too
   short", "heard nothing"), because Whisper invents text for silence. API errors show the API's
   `error.message`.
4. **Where the text goes.** Whitespace collapses to single spaces, because a line break would be
   Enter and send the text to an agent. When the text arrives, it is typed into the text field
   that has the keyboard exactly like an IME commit (every field of §13 and the Files editor). If
   a terminal pane has focus instead, it is pasted there like a paste (bracketed when the program
   asked for it, never followed by Enter). If neither has focus, it goes to the clipboard and the
   status bar says so.
5. Nothing is recorded or sent unless the user starts a recording, and audio is never stored.
```

- [ ] **Step 10: HUMAN_CHECKS and PROGRESS**

In `docs/HUMAN_CHECKS.md`, under `## To verify (non-blocking)`, add above the first `### To verify — 2026-10-10 …` section:

```markdown
### To verify — 2026-10-10 voice input (plan docs/superpowers/plans/2026-10-10-voice-input.md)

- [ ] **Setup:** run `setx GROQ_API_KEY gsk_…` in a terminal, then quit and restart Mkrew. In
  Windows Settings → Privacy → Microphone, "Let desktop apps access your microphone" is on.
- [ ] **Composer:** open the composer (Ctrl+Shift+Enter) and press Ctrl+Shift+D. The status bar
  shows `● 0:01 · Ctrl+Shift+D stops`, counting up. Say "Sửa lỗi đăng nhập rồi chạy test lại".
  Press Ctrl+Shift+D: the status bar shows `◌ Transcribing…`. Then the Vietnamese text, with
  diacritics, appears at the end of the draft and the status item disappears.
- [ ] **Accented English and mixed:** repeat with "Please refactor the login handler and add a
  test", then "Commit cái này rồi push lên branch main". English words stay English.
- [ ] **Terminal:** focus an agent pane and dictate a sentence. It appears in the agent's input
  as one paste and is **not** sent (no Enter).
- [ ] **Other fields:** dictate into the task dialog prompt, the Git commit box, the sidebar
  filter and the Files editor. The text lands in the focused one.
- [ ] **No target:** focus the Git file list and dictate. Nothing is staged or toggled, and the
  status bar says the text is on the clipboard. Ctrl+V pastes it into any field.
- [ ] **Slips:** press Ctrl+Shift+D twice quickly → `✕ Voice: too short…`. Record 3 s of silence
  → `✕ Voice: heard nothing…`. Nothing is typed in either case.
- [ ] **Errors:** run `setx GROQ_API_KEY ""` and restart → `✕ Voice: set the GROQ_API_KEY
  environment variable…`, and no recording starts. A wrong key gives `✕ Voice: HTTP 401: Invalid
  API Key`. With Wi-Fi off: `✕ Voice: cannot reach …`.
- [ ] **Auto stop:** set `[voice] max_seconds = 5` in settings.toml, start recording and keep
  talking. After 5 s it stops and transcribes by itself.
- [ ] **Other provider:** set `[voice] base_url = "https://api.openai.com/v1"`,
  `model = "gpt-4o-mini-transcribe"` and `api_key_env = "OPENAI_API_KEY"` (with that variable
  set). Voice input works the same.
```

Under `## Questions with assumed defaults`, add as the first bullet below its italic intro line:

```markdown
- 2026-10-10 voice input (ADR 0045): a toggle (press, then press again) instead of
  hold-to-talk, because the UI has no key-up handling. Shortcut Ctrl+Shift+D. The key is read
  from an environment variable, never from settings.toml. The language is detected per
  recording. The text goes to whatever has the keyboard when it arrives, else to the clipboard.
  Override in keymap.toml / `[voice]`.
```

In `docs/PROGRESS.md`, append under `## Notes for the next session`:

```markdown
- 2026-10-10 voice input (plan docs/superpowers/plans/2026-10-10-voice-input.md): `[voice]` settings, `voice.rs` (cpal), `stt.rs` (ureq → OpenAI-compatible `/audio/transcriptions`, Groq default), `Command::VoiceInput` Ctrl+Shift+D, SPEC §20G, ADR 0045; no protocol or migration change. Manual checks in HUMAN_CHECKS "2026-10-10 voice input". Outside the milestone loop.
```

- [ ] **Step 11: Full verify and commit**

```bash
sh scripts/verify.sh
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/ime.rs crates/mkrew-ui/src/main.rs docs/decisions/0045-voice-input.md docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git commit -m "feat(ui): voice input types speech into the focused field (Ctrl+Shift+D)"
```

Expected: verify is green (fmt, clippy `-D warnings`, all workspace tests).

- [ ] **Step 12: Smoke run on the real app**

Build and start the app from the worktree (`cargo run -p mkrew-ui --bin mkrew -j 4`). The UI connects to the daemon that is already running, because the protocol does not change; otherwise start the daemon from the worktree's `target\debug`. With `GROQ_API_KEY` set in that shell, run the "Composer" and "Terminal" checks from HUMAN_CHECKS and report what you saw. If no microphone is available in the session, say so and leave those checks open.
