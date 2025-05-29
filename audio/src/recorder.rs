use std::default;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleRate, StreamConfig};
use hound::SampleFormat;
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
    fn on_data(&self, data: Vec<i16>) -> RecorderResult<()>;
}

pub struct ChannelRecorderOutput {
    pub sender: Sender<Vec<i16>>,
}
impl RecorderOutput for ChannelRecorderOutput {
    fn on_data(&self, data: Vec<i16>) -> RecorderResult<()> {
        self.sender.send(data).map_err(|_| RecorderError::Unknown)
    }
}

pub struct OutputFormat {
    pub channels: RecorderChannelCount,
    pub sample_rate: RecorderSampleRate,
    pub sample_format: RecorderSampleFormat,
}

pub trait Recorder {
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

impl CpalRecorder {
    pub fn get_default_device() -> RecorderResult<(cpal::Device, cpal::SupportedStreamConfig)> {
        #[cfg(target_os = "macos")]
        {
            let host = cpal::host_from_id(cpal::HostId::ScreenCaptureKit)?;
            let device = host
                .default_input_device()
                .expect("No output devices found");

            let config = device
                .default_input_config()
                .expect("Not found default input config");
            return Ok((device, config));
        }
        #[cfg(not(target_os = "macos"))]
        {
            let host = cpal::default_host();
            let device = host
                .default_output_device()
                .expect("No output devices found");
            let config = device
                .default_output_config()
                .expect("Not found default output config");
            return Ok((device, config));
        }
    }
}

impl Recorder for CpalRecorder {
    fn start(&mut self, output: ChannelRecorderOutput) -> RecorderResult<()> {
        if self.stream.is_some() {
            return Err(RecorderError::Unknown);
        }
        let (device, config) = CpalRecorder::get_default_device()?;

        println!(
            "Using device: {} config: {} channels, {} Hz, {:?}",
            device.name().unwrap_or_else(|_| "Unknown".to_string()),
            config.channels(),
            config.sample_rate().0,
            config.sample_format()
        );

        let stream = device.build_input_stream(
            &config.config(),
            move |data: &[f32], _| {
                let mut data = data.to_vec();
                // If config is multi-channel, need to convert to single-channel
                if config.channels() > 1 {
                    data = data
                        .chunks_exact(2) // 每2个样本为一组（左、右声道）
                        .map(|chunk| (chunk[0] + chunk[1]) / 2.0) // 取平均值
                        .collect();
                }
                // Process audio data here
                let sample_data = data
                    .iter()
                    .map(|&s| {
                        return i16::from_sample(s.clone());
                    })
                    .collect::<Vec<i16>>();
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
