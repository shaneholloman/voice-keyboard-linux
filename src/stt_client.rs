use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordInfo {
    pub word: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub event: String,
    pub turn_index: u32,
    pub start: f64,
    pub timestamp: f64,
    pub transcript: String,
    pub words: Vec<WordInfo>,
    pub end_of_turn_confidence: f64,
}

pub struct SttClient {
    url: String,
    sample_rate: u32,
}

impl SttClient {
    pub fn new(url: &str, sample_rate: u32) -> Self {
        Self {
            url: url.to_string(),
            sample_rate,
        }
    }

    pub async fn connect_and_transcribe<F>(
        &self,
        mut on_transcription: F,
    ) -> Result<(mpsc::Sender<Vec<u8>>, tokio::task::JoinHandle<Result<()>>)>
    where
        F: FnMut(TranscriptionResult) + Send + 'static,
    {
        // Build WebSocket URL with query parameters
        let ws_url = format!(
            "{}?sample_rate={}&preflight_threshold=0.8&eot_threshold=0.7&eot_timeout_ms=3000",
            self.url, self.sample_rate
        );

        debug!("Connecting to speech-to-text service: {}", ws_url);

        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .context("Failed to connect to WebSocket")?;

        debug!("Connected to speech-to-text service");

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Create channel for sending audio data
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(32);

        // Spawn task to handle WebSocket communication
        let handle = tokio::spawn(async move {
            // Task to send audio data
            let send_task = tokio::spawn(async move {
                while let Some(audio_data) = audio_rx.recv().await {
                    if let Err(e) = ws_sender.send(Message::Binary(audio_data)).await {
                        error!("Failed to send audio data: {}", e);
                        break;
                    }
                }

                // Close the WebSocket when done
                let _ = ws_sender.close().await;
            });

            // Task to receive transcription results
            let receive_task = tokio::spawn(async move {
                while let Some(msg) = ws_receiver.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            debug!("Received text message: {}", text);

                            match serde_json::from_str::<TranscriptionResult>(&text) {
                                Ok(result) => {
                                    on_transcription(result);
                                }
                                Err(e) => {
                                    warn!("Failed to parse transcription result: {}", e);
                                }
                            }
                        }
                        Ok(Message::Binary(data)) => {
                            // Try to parse binary data as UTF-8 text first (for JSON responses)
                            match String::from_utf8(data.clone()) {
                                Ok(text) => {
                                    debug!("Received binary message as text: {}", text);

                                    match serde_json::from_str::<TranscriptionResult>(&text) {
                                        Ok(result) => {
                                            on_transcription(result);
                                        }
                                        Err(e) => {
                                            warn!("Failed to parse binary message as JSON: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    // Show first few bytes for debugging
                                    let preview = if data.len() > 10 {
                                        format!("{:?}...", &data[..10])
                                    } else {
                                        format!("{:?}", data)
                                    };
                                    debug!("Received binary message: {} bytes (not UTF-8 text, UTF-8 error: {}, preview: {})", 
                                          data.len(), e, preview);
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            debug!("WebSocket closed by server");
                            break;
                        }
                        Err(e) => {
                            error!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            });

            // Wait for either task to complete
            tokio::select! {
                _ = send_task => {
                    debug!("Audio sending task completed");
                }
                _ = receive_task => {
                    debug!("Transcription receiving task completed");
                }
            }

            Ok(())
        });

        Ok((audio_tx, handle))
    }
}

pub struct AudioBuffer {
    buffer: Vec<u8>,
    chunk_size: usize,
}

impl AudioBuffer {
    pub fn new(sample_rate: u32, chunk_duration_ms: u32) -> Self {
        // Calculate chunk size for 16-bit PCM audio
        // chunk_size = sample_rate * (chunk_duration_ms / 1000) * 2 bytes per sample
        let chunk_size = (sample_rate * chunk_duration_ms / 1000 * 2) as usize;

        debug!(
            "AudioBuffer: sample_rate={}, chunk_duration_ms={}, calculated chunk_size={} bytes",
            sample_rate, chunk_duration_ms, chunk_size
        );

        Self {
            buffer: Vec::new(),
            chunk_size,
        }
    }

    pub fn add_samples(&mut self, samples: &[f32]) -> Vec<Vec<u8>> {
        // Convert f32 samples to 16-bit PCM
        let pcm_data: Vec<u8> = samples
            .iter()
            .flat_map(|&sample| {
                let pcm_sample = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                pcm_sample.to_le_bytes()
            })
            .collect();

        self.buffer.extend_from_slice(&pcm_data);

        // Extract complete chunks
        let mut chunks = Vec::new();
        while self.buffer.len() >= self.chunk_size {
            let chunk: Vec<u8> = self.buffer.drain(..self.chunk_size).collect();
            chunks.push(chunk);
        }

        if !chunks.is_empty() {
            debug!(
                "Created {} audio chunks of {} bytes each (buffer size: {}, chunk_size: {})",
                chunks.len(),
                chunks[0].len(),
                self.buffer.len(),
                self.chunk_size
            );
        }

        chunks
    }

    #[allow(dead_code)]
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        if !self.buffer.is_empty() {
            let remaining = self.buffer.drain(..).collect();
            Some(remaining)
        } else {
            None
        }
    }
}
