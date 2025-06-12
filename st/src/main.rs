use audio::recorder::{CpalRecorder, Recorder};
use audio::wav::Wav;
use env_logger;
use gummy::Gummy;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::env::var;
use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use std::{sync::mpsc::channel, thread::spawn};
use tokio::runtime::Builder;

mod gummy;

fn main() {
    env_logger::init();
    let wav_path = format!(
        "{}{}",
        std::env::current_dir().unwrap().display(),
        "/target/recorded.wav"
    );
    let pcm_path = format!(
        "{}{}",
        std::env::current_dir().unwrap().display(),
        "/target/recorded.pcm"
    );
    let mut recorder = CpalRecorder::default();
    let recorder_format = CpalRecorder::output_format();
    let (tx, rx) = channel();
    let output = audio::recorder::ChannelRecorderOutput { sender: tx };
    let wav = Arc::new(Mutex::new(Some(Wav::new(&wav_path, &recorder_format))));
    let wav_cloned = Arc::clone(&wav);
    debug!("Recorder format: {:?}", recorder_format);
    spawn(move || {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        let mut file = fs::File::create(&pcm_path).expect("Failed to create WAV file");
        rt.block_on(async {
            let mut buffer = Vec::new();
            let api_key = var("API_KEY").expect("API_KEY environment variable not set");
            let gummy = Gummy::new(&api_key);
            let gummy = gummy
                .connect(None)
                .await
                .expect("Failed to connect to Gummy WebSocket");
            let mut gummy = gummy
                .start(
                    Some("pcm"),
                    Some(recorder_format.sample_rate),
                    Some("zh"),
                    None,
                )
                .await
                .unwrap();
            let mut count = 0.0;
            while let Ok(data) = rx.recv() {
                //debug!(
                //     "[{}] Received {} bytes of audio data",
                //     chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                //     data.len(),
                // );
                if let Ok(mut wav) = wav_cloned.lock() {
                    if let Some(wav) = wav.as_mut() {
                        wav.write::<i16, i16>(&data)
                            .expect("Failed to write to WAV");
                    }
                } else {
                    error!("Failed to lock WAV writer");
                }

                // Write PCM data to file
                file.write(
                    &data
                        .iter()
                        .map(|s| s.to_le_bytes())
                        .flatten()
                        .collect::<Vec<u8>>(),
                )
                .expect("Failed to write PCM data to file");
                buffer.extend_from_slice(&data);
                let duration_per_sample: f32 = recorder_format.sample_rate as f32
                    * (recorder_format.sample_format.sample_size()) as f32
                    * recorder_format.channels as f32;
                //debug!("Duration per sample: {} bytes", duration_per_sample);
                let buffer_duration = buffer.len() as f32 / duration_per_sample;
                //debug!("Buffer duration: {} s", buffer_duration);

                if buffer_duration >= 0.1 {
                    debug!("Buffer duration exceeded 100ms, processing...");
                    // Here you can process the buffer, e.g., send to Gummy
                    gummy
                        .send(
                            &buffer
                                .iter()
                                .map(|s| s.to_le_bytes())
                                .flatten()
                                .collect::<Vec<u8>>(),
                        )
                        .await
                        .unwrap();

                    buffer.clear();
                    count += buffer_duration;
                }
                debug!("Count is now: {}", count);
                if count > 5.0 {
                    let received_gummy = gummy.finish().await.unwrap();
                    debug!(
                        "[{}] Gummy task finished with ID: {:?}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        received_gummy.get_result()
                    );
                    // gummy = received_gummy.start().await.unwrap();
                    break;
                }
            }
        });
    });

    recorder.start(output).expect("Failed to start recorder");
    debug!("Recorder started, waiting for audio data...");
    sleep(Duration::from_secs(11));
    recorder.stop().expect("Failed to stop recorder");
    let mut wav = wav.lock().expect("Failed to lock WAV writer");
    if let Some(wav) = wav.take() {
        wav.save().expect("Failed to save WAV file");
        debug!("WAV file saved to {}", wav_path);
    } else {
        error!("WAV writer was already taken");
    }
}
