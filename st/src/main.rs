use audio::recorder::CpalRecorder;
use audio::wav::Wav;
use env_logger;
use gummy::Gummy;
use log::{debug, error};
use std::env::var;
use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use std::{sync::mpsc::channel, thread::spawn};
use tokio::runtime::Builder;
use tokio::select;

mod gummy;

#[tokio::main]
async fn main() {
    env_logger::init();
    let recorder = CpalRecorder::default();
    let recorder_format = CpalRecorder::output_format();
    debug!("Recorder format: {:?}", recorder_format);

    let mut recorder = recorder.start().expect("Failed to start recorder");

    let api_key = var("API_KEY").expect("API_KEY environment variable not set");
    let gummy = Gummy::new(&api_key);
    let gummy = gummy
        .connect(None)
        .await
        .expect("Failed to connect to Gummy WebSocket");
    let mut gummy = gummy
        .start(Some("pcm"), Some(recorder_format.sample_rate), None, None)
        .await
        .unwrap();

    loop {
        select! {
            sample_data= recorder.reveice_sample_data() => {
                if let Some(data) = sample_data {
                    // debug!("Received sample data: {}", data.data.len());
                    gummy
                        .send(
                            &data.data
                                .iter()
                                .map(|s| s.to_le_bytes())
                                .flatten()
                                .collect::<Vec<u8>>(),
                        )
                        .await
                        .unwrap();
                }
            },
            result = gummy.receive() => {
                if let Ok(data) = result {
                    debug!("Received recognition result: {}", data.len());
                    debug!("Message: {:?}",  data);
                    
                }
            }
        }
    }
    recorder.stop().expect("Failed to stop recorder");
}
