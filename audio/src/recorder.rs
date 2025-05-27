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
    fn output_format() -> RecorderResult<OutputFormat>;
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
    pub fn get_default_device() -> RecorderResult<cpal::Device> {
        #[cfg(target_os = "macos")]
        {
            let host = cpal::host_from_id(cpal::HostId::ScreenCaptureKit)?;
            let device = host
                .default_input_device()
                .expect("No output devices found");
        }
        #[cfg(not(target_os = "macos"))]
        {
            let host = cpal::default_host();
            let device = host
                .default_output_device()
                .expect("No output devices found");
            return Ok(device);
        }
    }

}

impl Recorder for CpalRecorder {
    fn start(&mut self, output: ChannelRecorderOutput) -> RecorderResult<()> {
        if self.stream.is_some() {
            return Err(RecorderError::Unknown);
        }
        let device = CpalRecorder::get_default_device()?;
        let default_config = CpalRecorder::output_format()?;
        let config = StreamConfig {
            channels: 1,
            sample_rate: SampleRate(default_config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        println!(
            "Using device: {}",
            device.name().unwrap_or_else(|_| "Unknown".to_string())
        );

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _| {
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

    fn output_format() -> RecorderResult<OutputFormat> {
        let device = CpalRecorder::get_default_device().expect("Failed to get default device");
        #[cfg(target_os = "macos")]
        let config = device
            .default_input_config()
            .expect("Not found default input config");
        #[cfg(not(target_os = "macos"))]
        let config = device
            .default_output_config()
            .expect("Not found default output config");
        println!(
            "Default device: {}, channels: {}, sample rate: {}, sample format: {:?}",
            device.name().unwrap_or_else(|_| "Unknown".to_string()),
            config.channels(),
            config.sample_rate().0,
            config.sample_format()
        );
        Ok(OutputFormat {
            channels: config.channels() as RecorderChannelCount,
            sample_rate: config.sample_rate().0 as RecorderSampleRate,
            sample_format: config.sample_format(),
        })
    }
}
