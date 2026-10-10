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
- Recordings under 0.3 s, or with less than 0.15 s of sound above 1 % of full scale (RMS over
  30 ms), are not sent: Whisper invents text for silence.
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
- Voice text reaches a field like an IME commit, one char at a time; in the Files editor it
  joins the current typing run as one undo step.
