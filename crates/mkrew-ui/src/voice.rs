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
/// A 30 ms frame with an RMS below this (about -40 dBFS) is silence; Whisper invents text for
/// silence.
pub const MIN_LEVEL: f32 = 0.01;
/// A clip with less sound than this (in frames at or above `MIN_LEVEL`) is not sent: one key
/// click (the shortcut press itself) must not pass for speech.
pub const MIN_VOICED_SECONDS: f32 = 0.15;

/// The shell's voice input.
#[derive(Default)]
pub enum State {
    #[default]
    Idle,
    /// Leaving this state drops `stop`, which ends the recording.
    Recording {
        since: Instant,
        stop: Stop,
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
            .filter(|frame| {
                let energy = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
                energy.sqrt() >= MIN_LEVEL
            })
            .map(<[f32]>::len)
            .sum();
        if (voiced as f32 / self.rate.max(1) as f32) < MIN_VOICED_SECONDS {
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
        let _ = failed.try_send(Err(
            anyhow!(error).context("cannot start the microphone thread")
        ));
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
