//! Voice input capture (SPEC §20G): the default microphone on its own thread, mixed to mono,
//! and the 16-bit WAV sent for speech to text.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, anyhow, bail};
use cpal::FromSample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::theme;

/// Shorter recordings are a slip of the shortcut, not speech.
pub const MIN_SECONDS: f32 = 0.3;
/// A 30 ms frame with an RMS below this (about -40 dBFS) is silence; Whisper invents text for
/// silence.
pub const MIN_LEVEL: f32 = 0.01;
/// A clip with less sound than this (in frames at or above `MIN_LEVEL`) is not sent: one key
/// click (the shortcut press itself) must not pass for speech.
pub const MIN_VOICED_SECONDS: f32 = 0.15;
/// The level meter is empty at this loudness (dBFS) and full at `METER_FULL_DB`: the range of
/// normal speech into a laptop or headset microphone.
const METER_FLOOR_DB: f32 = -45.0;
const METER_FULL_DB: f32 = -15.0;

/// The shell's voice input.
#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    /// Leaving this state drops `stop`, which ends the recording. `shown` is the meter as
    /// drawn: [`smooth`]ed toward `level` on every meter tick.
    Recording {
        since: Instant,
        stop: Stop,
        level: Level,
        shown: f32,
    },
    Transcribing,
    /// The last outcome worth showing (an error, or where the text went); the next start
    /// clears it.
    Message(String),
}

/// Ends the recording when dropped (the capture thread waits on the other end).
pub struct Stop {
    _alive: mpsc::Sender<()>,
}

/// The microphone's loudness right now (RMS of the last block it delivered), for the meter.
#[derive(Clone, Default)]
pub struct Level(Arc<AtomicU32>);

impl Level {
    fn set(&self, rms: f32) {
        self.0.store(rms.to_bits(), Ordering::Relaxed);
    }

    /// How full the meter is, 0..=1.
    pub fn meter(&self) -> f32 {
        meter(f32::from_bits(self.0.load(Ordering::Relaxed)))
    }
}

/// Maps an RMS level to 0..=1 on a dB scale (loudness is heard logarithmically).
pub fn meter(rms: f32) -> f32 {
    let db = 20.0 * rms.max(f32::MIN_POSITIVE).log10();
    ((db - METER_FLOOR_DB) / (METER_FULL_DB - METER_FLOOR_DB)).clamp(0.0, 1.0)
}

/// One meter tick of the drawn level toward `target` (both 0..=1): it jumps up with the voice
/// and sinks slowly, so the bars follow speech without flicker.
pub fn smooth(shown: f32, target: f32) -> f32 {
    let rate = if target > shown {
        theme::VOICE_RISE
    } else {
        theme::VOICE_FALL
    };
    shown + (target - shown) * rate
}

/// How tall each waveform bar is (0..=1) at drawn level `level`, `elapsed` into the
/// recording: tallest in the middle, a wave running across them, and a low breath in silence
/// so a live microphone never looks still. `elapsed: None` (reduce motion) stands still.
pub fn bars(level: f32, elapsed: Option<Duration>) -> [f32; 7] {
    const SHAPE: [f32; 7] = [0.45, 0.7, 0.9, 1.0, 0.9, 0.7, 0.45];
    let full = theme::VOICE_IDLE + (1.0 - theme::VOICE_IDLE) * level;
    let turn = elapsed.map(|t| t.as_secs_f32() / theme::VOICE_WAVE_PERIOD.as_secs_f32());
    let mut out = SHAPE;
    for (i, bar) in out.iter_mut().enumerate() {
        // 0..=1 per bar, each a step behind its left neighbour.
        let wave = turn.map_or(1.0, |turn| {
            0.5 + 0.5 * (std::f32::consts::TAU * turn - i as f32 * theme::VOICE_WAVE_STEP).cos()
        });
        *bar *= full * (1.0 - theme::VOICE_WAVE_DEPTH + theme::VOICE_WAVE_DEPTH * wave);
    }
    out
}

/// The opacity of a breathing mark `elapsed` into it: full, down to
/// [`theme::PULSE_MIN_OPACITY`] half a [`theme::PULSE_PERIOD`] later, and back.
pub fn pulse(elapsed: Duration) -> f32 {
    let turn = elapsed.as_secs_f32() / theme::PULSE_PERIOD.as_secs_f32();
    let dip = 0.5 - 0.5 * (std::f32::consts::TAU * turn).cos();
    1.0 - (1.0 - theme::PULSE_MIN_OPACITY) * dip
}

/// Root mean square of `samples` (0 for none).
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
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
        let frame = (self.rate as usize * 30 / 1000).max(1);
        let voiced: usize = self
            .samples
            .chunks(frame)
            .filter(|frame| rms(frame) >= MIN_LEVEL)
            .map(<[f32]>::len)
            .sum();
        if (voiced as f32 / self.rate.max(1) as f32) < MIN_VOICED_SECONDS {
            bail!("heard nothing: is the microphone muted?");
        }
        Ok(wav(&self.samples, self.rate))
    }
}

/// Starts recording the default microphone until `Stop` is dropped or `max` passes. `Level`
/// follows the loudness while it records; the clip (or why there is none) arrives once on the
/// returned channel.
pub fn record(max: Duration) -> (Stop, Level, async_channel::Receiver<Result<Clip>>) {
    let level = Level::default();
    let meter = level.clone();
    let (alive, stopped) = mpsc::channel::<()>();
    let (done, clip) = async_channel::bounded(1);
    let failed = done.clone();
    let spawned = std::thread::Builder::new()
        .name("mkrew-voice".into())
        .spawn(move || {
            let _ = done.send_blocking(capture(&stopped, max, &meter));
        });
    if let Err(error) = spawned {
        let _ = failed.try_send(Err(
            anyhow!(error).context("cannot start the microphone thread")
        ));
    }
    (Stop { _alive: alive }, level, clip)
}

fn capture(stopped: &mpsc::Receiver<()>, max: Duration, level: &Level) -> Result<Clip> {
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
        cpal::SampleFormat::F32 => input::<f32>(&device, config, channels, &samples, level),
        cpal::SampleFormat::I16 => input::<i16>(&device, config, channels, &samples, level),
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
    sink: &Arc<Mutex<Vec<f32>>>,
    level: &Level,
) -> Result<cpal::Stream, cpal::Error>
where
    T: cpal::SizedSample,
    f32: FromSample<T>,
{
    let (sink, level) = (sink.clone(), level.clone());
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if let Ok(mut out) = sink.lock() {
                let start = out.len();
                push_mono(data, channels, &mut out);
                level.set(rms(&out[start..]));
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
    fn meter_spans_speech_on_a_db_scale() {
        assert_eq!(meter(0.0), 0.0, "silence is empty");
        assert_eq!(meter(1.0), 1.0, "full scale is full");
        // -45 dBFS empty, -15 dBFS full, -30 dBFS half way.
        assert!(meter(10f32.powf(-45.0 / 20.0)) < 1e-4);
        assert!((meter(10f32.powf(-15.0 / 20.0)) - 1.0).abs() < 1e-4);
        assert!((meter(10f32.powf(-30.0 / 20.0)) - 0.5).abs() < 1e-4);
        // The silence gate (MIN_LEVEL, -40 dBFS) shows as a sliver, not as speech.
        assert!(meter(MIN_LEVEL) < 0.2);
    }

    #[test]
    fn smoothing_rises_fast_and_falls_slow() {
        let up = smooth(0.0, 1.0);
        let down = 1.0 - smooth(1.0, 0.0);
        assert!(up > down, "rise {up} beats fall {down}");
        assert_eq!(smooth(0.4, 0.4), 0.4, "at the target it stays");
        // Many ticks of silence bring it all the way down, never below.
        let rest = (0..200).fold(1.0, |shown, _| smooth(shown, 0.0));
        assert!((0.0..1e-3).contains(&rest), "{rest}");
    }

    #[test]
    fn bars_wave_breathe_in_silence_and_rise_with_the_voice() {
        let at = |ms| bars(0.0, Some(Duration::from_millis(ms)));
        // Silence still shows low bars, and they move.
        assert!(at(0).iter().all(|&bar| bar > 0.0), "{:?}", at(0));
        assert_ne!(at(0), at(200));
        // Neighbours are out of step: the middle pair (same shape weight) differs.
        let frame = bars(1.0, Some(Duration::from_millis(200)));
        assert!((frame[2] - frame[4]).abs() > 1e-3, "{frame:?}");
        // Louder is taller, and full voice reaches the top in the middle.
        assert!(bars(1.0, None)[3] > bars(0.2, None)[3]);
        assert_eq!(bars(1.0, None)[3], 1.0);
        assert!(bars(1.0, None).iter().all(|&bar| bar <= 1.0));
    }

    #[test]
    fn pulse_dips_to_the_minimum_half_way_and_comes_back() {
        assert_eq!(pulse(Duration::ZERO), 1.0);
        let half = pulse(theme::PULSE_PERIOD / 2);
        assert!((half - theme::PULSE_MIN_OPACITY).abs() < 1e-4, "{half}");
        assert!((pulse(theme::PULSE_PERIOD) - 1.0).abs() < 1e-4);
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
        // The level is by magnitude: a negative-only signal is speech too.
        assert_eq!(clip(2000, -0.5).into_wav().unwrap().len(), 44 + 4000);
        // Exactly the minimum length and just above the minimum level are sent.
        assert_eq!(clip(300, 0.02).into_wav().unwrap().len(), 44 + 600);
        // Exactly the minimum sound (5 frames of 30 ms = 0.15 s) in a quiet second is sent.
        let mut samples = vec![0.0; 1000];
        samples[..150].fill(0.5);
        assert_eq!(
            Clip {
                samples,
                rate: 1000
            }
            .into_wav()
            .unwrap()
            .len(),
            44 + 2000
        );
        // One frame short of it is not.
        let mut samples = vec![0.0; 1000];
        samples[..120].fill(0.5);
        let few = Clip {
            samples,
            rate: 1000,
        }
        .into_wav()
        .unwrap_err()
        .to_string();
        assert!(few.contains("heard nothing"), "{few}");
        // A key click (a 10 ms spike) in 2 s of silence is not speech.
        let mut samples = vec![0.0; 2000];
        samples[500..510].fill(0.9);
        let click = Clip {
            samples,
            rate: 1000,
        }
        .into_wav()
        .unwrap_err()
        .to_string();
        assert!(click.contains("heard nothing"), "{click}");
    }
}
