use anyhow::{Context, Result};
use clap::{Arg, Command};
use nix::unistd::{getgid, getuid, setgid, setuid, Gid, Uid};
use std::env;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tracing::{error, info};

mod audio_input;
mod input_event;
mod stt_client;
mod virtual_keyboard;

use audio_input::AudioInput;
use stt_client::{AudioBuffer, SttClient};
use virtual_keyboard::VirtualKeyboard;

#[derive(Debug)]
struct OriginalUser {
    uid: Uid,
    gid: Gid,
    home: Option<String>,
    user: Option<String>,
}

impl OriginalUser {
    fn capture() -> Self {
        // If we're running under sudo, get the original user info
        let uid = if let Ok(sudo_uid) = env::var("SUDO_UID") {
            Uid::from_raw(sudo_uid.parse().unwrap_or_else(|_| getuid().as_raw()))
        } else {
            getuid()
        };

        let gid = if let Ok(sudo_gid) = env::var("SUDO_GID") {
            Gid::from_raw(sudo_gid.parse().unwrap_or_else(|_| getgid().as_raw()))
        } else {
            getgid()
        };

        let home = env::var("HOME").ok();
        let user = env::var("SUDO_USER").ok().or_else(|| env::var("USER").ok());

        Self {
            uid,
            gid,
            home,
            user,
        }
    }

    fn drop_privileges(&self) -> Result<()> {
        if getuid().is_root() {
            info!(
                "Dropping root privileges to uid={}, gid={}",
                self.uid, self.gid
            );

            // Preserve important environment variables
            let pulse_runtime_path = env::var("PULSE_RUNTIME_PATH").ok();
            let xdg_runtime_dir = env::var("XDG_RUNTIME_DIR").ok();
            let display = env::var("DISPLAY").ok();
            let wayland_display = env::var("WAYLAND_DISPLAY").ok();

            // Drop group first, then user (required order)
            setgid(self.gid).context("Failed to drop group privileges")?;
            setuid(self.uid).context("Failed to drop user privileges")?;

            // Restore environment variables for the original user
            if let Some(ref home) = self.home {
                env::set_var("HOME", home);
            }
            if let Some(ref user) = self.user {
                env::set_var("USER", user);
            }

            // Restore audio-related environment variables
            if let Some(pulse_path) = pulse_runtime_path {
                env::set_var("PULSE_RUNTIME_PATH", pulse_path);
            }
            if let Some(xdg_path) = xdg_runtime_dir {
                env::set_var("XDG_RUNTIME_DIR", xdg_path);
            }
            if let Some(disp) = display {
                env::set_var("DISPLAY", disp);
            }
            if let Some(wayland_disp) = wayland_display {
                env::set_var("WAYLAND_DISPLAY", wayland_disp);
            }

            info!("Successfully dropped privileges to user");

            // Give audio system a moment to be ready
            std::thread::sleep(std::time::Duration::from_millis(100));
        } else {
            info!("Not running as root, no privilege dropping needed");
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Voice Keyboard v{}", env!("CARGO_PKG_VERSION"));

    // Capture original user info before we do anything
    let original_user = OriginalUser::capture();

    let matches = Command::new("voice-keyboard")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Voice-controlled keyboard input")
        .arg(
            Arg::new("test-audio")
                .long("test-audio")
                .help("Test audio input")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("test-stt")
                .long("test-stt")
                .help("Test speech-to-text functionality")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("stt-url")
                .long("stt-url")
                .help("Custom STT service URL")
                .value_name("URL")
                .default_value("ws://localhost:8765"),
        )
        .get_matches();

    let device_name = "Voice Keyboard";

    // Step 1: Create virtual keyboard while we have root privileges
    info!("Creating virtual keyboard device (requires root privileges)...");
    let keyboard =
        VirtualKeyboard::new(device_name).context("Failed to create virtual keyboard")?;
    info!("Virtual keyboard created successfully");

    // Step 2: Drop root privileges before initializing audio
    original_user
        .drop_privileges()
        .context("Failed to drop root privileges")?;

    if matches.get_flag("test-audio") {
        test_audio().await?;
    } else if matches.get_flag("test-stt") {
        let stt_url = matches.get_one::<String>("stt-url").unwrap();
        test_stt(keyboard, stt_url).await?;
    } else {
        info!("Voice Keyboard is ready. Use --test-audio or --test-stt to test functionality.");
    }

    Ok(())
}

async fn test_audio() -> Result<()> {
    info!("Testing audio input...");

    // List available devices
    info!("Available input devices:");
    let devices = AudioInput::list_available_devices()?;
    for (i, device) in devices.iter().enumerate() {
        info!("  {}: {}", i + 1, device);
    }

    // Create audio input
    let mut audio_input = AudioInput::new()?;
    info!(
        "Using audio device with {} channels at {} Hz",
        audio_input.get_channels(),
        audio_input.get_sample_rate()
    );

    // Test recording for 5 seconds
    let (tx, rx) = mpsc::channel();

    audio_input.start_recording(move |data| {
        let level = data.iter().map(|&x| x.abs()).sum::<f32>() / data.len() as f32;
        let _ = tx.send(level);
    })?;

    info!("Recording for 5 seconds...");
    let start = std::time::Instant::now();

    while start.elapsed() < Duration::from_secs(5) {
        if let Ok(level) = rx.try_recv() {
            let bar_length = (level * 50.0) as usize;
            let bar: String = "#".repeat(bar_length);
            info!("Level: {:.2} [{}]", level, bar);
        }
        thread::sleep(Duration::from_millis(50));
    }

    info!("Audio test completed!");
    Ok(())
}

async fn test_stt(keyboard: VirtualKeyboard, stt_url: &str) -> Result<()> {
    info!("Testing speech-to-text functionality...");
    info!("STT Service URL: {}", stt_url);

    // Create audio input after privilege drop
    let mut audio_input = AudioInput::new()?;
    let channels = audio_input.get_channels();
    let sample_rate = audio_input.get_sample_rate();

    info!(
        "Using audio device with {} channels at {} Hz",
        channels, sample_rate
    );

    // Create STT client
    let stt_client = SttClient::new(stt_url, sample_rate);

    // Connect to STT service
    let (audio_tx, stt_handle) = stt_client
        .connect_and_transcribe(move |result| {
            info!("Transcription [{}]: {}", result.event, result.transcript);

            // Type completed transcriptions (EndOfTurn events)
            if result.event == "EndOfTurn" && !result.transcript.is_empty() {
                info!("Typing: {}", result.transcript);
                if let Err(e) = keyboard.type_text(&result.transcript) {
                    error!("Failed to type text: {}", e);
                }
                if let Err(e) = keyboard.press_enter() {
                    error!("Failed to press enter: {}", e);
                }
            }
        })
        .await?;

    // Create audio buffer for chunking
    let mut audio_buffer = AudioBuffer::new(sample_rate, 80); // 80ms chunks

    info!("Listening for speech... Speak into your microphone!");
    info!("Press Ctrl+C to stop.");

    // Start audio recording
    let audio_tx_clone = audio_tx.clone();
    audio_input.start_recording(move |data| {
        // Average all channels together if multichannel
        let averaged_samples = if channels == 1 {
            // Mono audio - use as is
            data.to_vec()
        } else {
            // Multichannel audio - average channels together
            let num_frames = data.len() / channels as usize;
            let mut averaged = Vec::with_capacity(num_frames);

            for frame in 0..num_frames {
                let mut sum = 0.0;
                for channel in 0..channels as usize {
                    sum += data[frame * channels as usize + channel];
                }
                averaged.push(sum / channels as f32);
            }
            averaged
        };

        // Convert to chunks and send to STT
        let chunks = audio_buffer.add_samples(&averaged_samples);
        for chunk in chunks {
            if let Err(e) = audio_tx_clone.blocking_send(chunk) {
                error!("Failed to send audio chunk: {}", e);
            }
        }
    })?;

    // Keep the main thread alive
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");

    // Stop recording and clean up
    audio_input.stop_recording();
    drop(audio_tx); // Close the audio channel

    info!("Waiting for STT connection to close...");
    let _ = stt_handle.await;

    Ok(())
}
