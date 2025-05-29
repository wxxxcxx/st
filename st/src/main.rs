use audio::recorder::{CpalRecorder, OutputFormat, Recorder, RecorderSampleFormat};
use audio::wav::Wav;
use gummy::Gummy;
use serde::{Deserialize, Serialize};
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
    let (tx, rx) = channel();
    let output = audio::recorder::ChannelRecorderOutput { sender: tx };
    let wav = Arc::new(Mutex::new(Some(Wav::new(
        &wav_path,
        &OutputFormat {
            sample_format: RecorderSampleFormat::I16,
            channels: 1,
            sample_rate: 48000,
        },
    ))));
    let wav_cloned = Arc::clone(&wav);

    // let (ttx, trx) = tokio::sync::mpsc::channel(1024);

    // spawn(move || {
    //     let rt = Builder::new_multi_thread()
    //         .worker_threads(3)
    //         .enable_all()
    //         .build()
    //         .unwrap();
    //     rt.block_on(async move {
    //         let api_key = std::env::var("API_KEY").expect("API_KEY 未设置");
    //         let gummy = Gummy::connect(api_key).await.unwrap();
    //         match gummy {
    //             Gummy::Ready(_) => {
    //                 gummy.start().await.expect("Failed to start Gummy");
    //             }
    //             Gummy::Processing(_) => {
    //                 gummy.bind(trx).await.expect("Failed to bind Gummy");
    //             }
    //             Gummy::Closed => panic!("Gummy connection closed unexpectedly"),
    //             Gummy::Error(err) => panic!("Failed to connect: {}", err),
    //         };
    //     });
    // });

    spawn(move || {
        while let Ok(data) = rx.recv() {
            println!(
                "[{}] Received {} bytes of audio data",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                data.len(),
            );

            if let Ok(mut wav) = wav_cloned.lock() {
                if let Some(ref mut wav) = *wav {
                    wav.write::<i16, i16>(&data)
                        .expect("Failed to write to WAV");
                }
            } else {
                eprintln!("Failed to lock WAV writer");
            }

            let mut bytes = Vec::with_capacity(data.len() * 2);
            for &sample in data.iter() {
                bytes.extend_from_slice(&sample.to_ne_bytes());
            }
            // Builder::new_current_thread()
            //     .enable_all()
            //     .build()
            //     .unwrap()
            //     .block_on(async {
            //         ttx.send(bytes).await.expect("Failed to send data to Gummy");
            //     })
        }
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
}
