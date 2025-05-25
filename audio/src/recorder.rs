use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RecorderError {
    #[error("Failed to find host: {0}")]
    HostUnavailable(#[from] cpal::HostUnavailable),
    #[error("Failed to initialize audio recorder: {0}")]
    BuildStreamError(#[from] cpal::BuildStreamError),
    #[error("Failed to start audio recorder: {0}")]
    PlayStreamError(#[from] cpal::PlayStreamError),
    #[error("Failed to stop audio recorder: {0}")]
    PauseStreamError(#[from] cpal::PauseStreamError),
    #[error("Unknown error")]
    Unknown,
}

pub type RecorderResult<T> = std::result::Result<T, RecorderError>;

pub type RecorderChannelCount = u16;
pub type RecorderSampleRate = u32;
pub type RecorderSampleFormat = cpal::SampleFormat;

pub trait RecorderOutput {
    fn on_data(&self, data: Vec<i32>) -> RecorderResult<()>;
}

pub struct ChannelRecorderOutput {
    pub sender: Sender<Vec<i32>>,
}
impl RecorderOutput for ChannelRecorderOutput {
    fn on_data(&self, data: Vec<i32>) -> RecorderResult<()> {
        self.sender.send(data).map_err(|_| RecorderError::Unknown)
    }
}

pub struct AudioConfig {
    pub channels: RecorderChannelCount,
    pub sample_rate: RecorderSampleRate,
    pub sample_format: RecorderSampleFormat,
}

pub trait Recorder {
    fn default_config(&self) -> RecorderResult<AudioConfig>;
    fn start(&mut self, output: ChannelRecorderOutput) -> RecorderResult<()>;
    fn stop(&mut self) -> RecorderResult<()>;
}

pub struct CpalRecorder {
    stream: Option<cpal::Stream>,
}

impl Default for CpalRecorder {
    fn default() -> Self {
        CpalRecorder { stream: None }
    }
}

impl Recorder for CpalRecorder {
    fn default_config(&self) -> RecorderResult<AudioConfig> {
        let host = cpal::host_from_id(cpal::HostId::ScreenCaptureKit)?;
        // let host = cpal::default_host();
        let device = host
            .default_input_device()
            .expect("No output devices found");
        let config = device
            .default_input_config()
            .expect("Not found default input config");

        Ok(AudioConfig {
            channels: config.channels() as RecorderChannelCount,
            sample_rate: config.sample_rate().0 as RecorderSampleRate,
            sample_format: config.sample_format(),
        })
    }

    fn start(&mut self, output: ChannelRecorderOutput) -> RecorderResult<()> {
        if self.stream.is_some() {
            return Err(RecorderError::Unknown);
        }
        #[cfg(target_os = "macos")]
        let host = cpal::host_from_id(cpal::HostId::ScreenCaptureKit)?;
        // let host = cpal::default_host();
        let device = host
            .default_input_device()
            .expect("No output devices found");
        let config = device
            .default_input_config()
            .expect("Not found default input config");
        println!(
            "Using device: {}",
            device.name().unwrap_or_else(|_| "Unknown".to_string())
        );

        let stream = device.build_input_stream(
            &config.config(),
            move |data: &[f32], _| {
                // Process audio data here
                let sample_data = data.iter().map(|&s|{
                    return i32::from_sample(s.clone());
                }).collect::<Vec<i32>>();
                if let Err(e) = output.on_data(sample_data) {
                    eprintln!("Error sending data to output: {}", e);
                }
            },
            |err| {
                eprintln!("Error occurred on input stream: {}", err);
            },
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) -> RecorderResult<()> {
        println!("Stopping recorder...");
        if let Some(stream) = self.stream.take() {
            stream.pause()?;
            self.stream = None;
        }
        Ok(())
    }
}
