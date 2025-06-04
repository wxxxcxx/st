use audio::recorder::{CpalRecorder, OutputFormat, Recorder, RecorderSampleFormat};
use audio::wav::Wav;
use gummy::Gummy;
use serde::{Deserialize, Serialize};
use std::env::{var, vars};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use std::{sync::mpsc::channel, thread::spawn};
use tokio::runtime::Builder;

mod gummy;

#[derive(Serialize, Deserialize)]
pub struct ConvertMessageHeader {
    task_id: String,
    action: String,
    streaming: String,
}

impl ConvertMessageHeader {
    pub fn new(action: String) -> Self {
        let task_id = uuid::Uuid::new_v4().to_string();
        ConvertMessageHeader {
            task_id,
            action,
            streaming: "duplex".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ConvertMessagePayload {}
#[derive(Serialize, Deserialize)]
pub struct ConvertMessage {
    header: ConvertMessageHeader,
    payload: ConvertMessagePayload,
}

impl ConvertMessage {
    pub fn new(event: String) -> Self {
        ConvertMessage {
            header: ConvertMessageHeader::new(event),
            payload: ConvertMessagePayload {},
        }
    }
}

fn main() {
    let wav_path = format!(
        "{}{}",
        std::env::current_dir().unwrap().display(),
        "/target/recorded.wav"
    );
    let mut recorder = CpalRecorder::default();
    let recorder_format = CpalRecorder::output_format();
    let (tx, rx) = channel();
    let output = audio::recorder::ChannelRecorderOutput { sender: tx };
    let wav = Arc::new(Mutex::new(Some(Wav::new(&wav_path, &recorder_format))));
    let wav_cloned = Arc::clone(&wav);

    spawn(move || {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let mut buffer = Vec::new();
            let api_key = var("API_KEY").expect("API_KEY environment variable not set");
            let mut gummy: Gummy = Gummy::connect(api_key).await.unwrap();
            while let Ok(data) = rx.recv() {
                println!(
                    "[{}] Received {} bytes of audio data",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                    data.len(),
                );
                if let Ok(mut wav) = wav_cloned.lock() {
                    if let Some(wav) = wav.as_mut() {
                        wav.write::<i16, i16>(&data)
                            .expect("Failed to write to WAV");
                    }
                } else {
                    eprintln!("Failed to lock WAV writer");
                }
                buffer.extend_from_slice(&data);
                let duration_per_sample: f32 = recorder_format.sample_rate as f32
                    * (recorder_format.sample_format.sample_size()) as f32
                    * recorder_format.channels as f32;
                println!("Duration per sample: {} bytes", duration_per_sample);
                let buffer_duration = buffer.len() as f32 / duration_per_sample;
                println!("Buffer duration: {} s", buffer_duration);
                if buffer_duration >= 0.1 {
                    println!("Buffer duration exceeded 100ms, processing...");
                    // Here you can process the buffer, e.g., send to Gummy
                    gummy = gummy.start_task().await.unwrap();
                    gummy
                        .send_data(
                            &buffer
                                .iter()
                                .map(|s| s.to_le_bytes())
                                .flatten()
                                .collect::<Vec<u8>>(),
                        )
                        .await
                        .unwrap();
                    gummy.finish_task().await.unwrap();
                    buffer.clear();
                }
            }
        });
    });

    recorder.start(output).expect("Failed to start recorder");
    println!("Recorder started, waiting for audio data...");
    sleep(Duration::from_secs(10));
    recorder.stop().expect("Failed to stop recorder");
    let mut wav = wav.lock().expect("Failed to lock WAV writer");
    if let Some(wav) = wav.take() {
        wav.save().expect("Failed to save WAV file");
        println!("WAV file saved to {}", wav_path);
    } else {
        eprintln!("WAV writer was already taken");
    }
    sleep(Duration::from_secs(30));
}
