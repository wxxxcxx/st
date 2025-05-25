use audio::recorder::{CpalRecorder, Recorder};
use audio::wav::Wav;
use futures_util::{SinkExt, StreamExt, future, pin_mut};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::{sync::mpsc::channel, thread::spawn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal::unix::{SignalKind, signal};
use tokio::time::{Duration, sleep};
use tokio_tungstenite::{connect_async, connect_async_tls_with_config};
use tungstenite::Message;
use tungstenite::Utf8Bytes;
use tungstenite::client::IntoClientRequest;

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/recorded.wav");

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

#[tokio::main]
async fn main() {
    let mut recorder = CpalRecorder::default();
    let config = recorder
        .default_config()
        .expect("Failed to get default config");
    let (tx, rx) = channel();
    let output = audio::recorder::ChannelRecorderOutput { sender: tx };
    let wav = Arc::new(Mutex::new(Some(Wav::new(PATH, &config))));
    let wav_cloned = Arc::clone(&wav);

    // let (mut ftx, frx) = futures_channel::mpsc::channel(1024);

    let api_key = std::env::var("API_KEY").expect("API_KEY 未设置");
    let mut request = "wss://dashscope.aliyuncs.com/api-ws/v1/inference"
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", api_key).parse().unwrap(),
    );
    request
        .headers_mut()
        .insert("user-agent", "app".parse().unwrap());
    request
        .headers_mut()
        .insert("X-DashScope-WorkSpace", "app".parse().unwrap());
    request
        .headers_mut()
        .insert("X-DashScope-DataInspection", "enable".parse().unwrap());
    let (stream, _) = connect_async_tls_with_config(request, None, false, None)
        .await
        .expect("Failed to connect WebSocket");
    let (mut writer, reader) = stream.split();
    let message = ConvertMessage::new("run-task".to_string());
    let message = serde_json::to_string(&message).expect("Failed to serialize message");
    println!(
        "[{}] Sending message: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        message
    );
    writer
        .send(Message::Text(message.into()))
        .await
        .expect("Failed to send message");

    reader
        .for_each(|message| async {
            match message {
                Ok(Message::Binary(data)) => {
                    println!(
                        "[{}] Received {} bytes of audio data",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        data.len(),
                    );
                }
                Ok(Message::Text(data)) => {
                    println!(
                        "[{}] Received text message: {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        data
                    );
                }
                Ok(Message::Close(_)) => {
                    println!("WebSocket connection closed");
                }
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                }
                _ => {}
            }
        })
        .await;

    // spawn(move || {
    //     let mut count = 0;
    //     while let Ok(data) = rx.recv() {
    //         count += 1;
    //         println!(
    //             "{} [{}] Received {} bytes of audio data",
    //             count,
    //             chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
    //             data.len(),
    //         );
    //         ftx.try_send(data.clone())
    //             .expect("Failed to send data to WebSocket");
    //         if let Ok(mut wav) = wav_cloned.lock() {
    //             if let Some(ref mut wav) = *wav {
    //                 wav.write::<i32, f32>(&data)
    //                     .expect("Failed to write to WAV");
    //             }
    //         } else {
    //             eprintln!("Failed to lock WAV writer");
    //         }
    //     }
    // });
    // recorder.start(output).expect("Failed to start recorder");
    // let mut sigint = signal(SignalKind::interrupt()).expect("无法注册信号处理器");
    // tokio::spawn(async {
    //     loop {
    //         sleep(Duration::from_secs(2)).await;
    //     }
    // });
    // // 等待信号
    // sigint.recv().await;
    // recorder.stop().expect("Failed to stop recorder");

    // let mut wav = wav.lock().expect("Failed to lock WAV writer");
    // if let Some(wav) = wav.take() {
    //     wav.save().expect("Failed to save WAV file");
    //     println!("WAV file saved to {}", PATH);
    // } else {
    //     eprintln!("WAV writer was already taken");
    // }
}
