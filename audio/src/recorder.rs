use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{debug, error};
use std::sync::mpsc::Sender;
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
    #[error("Failed to send audio data: {0}")]
    SenderError(#[from] std::sync::mpsc::SendError<Vec<i16>>),
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
        Ok(self.sender.send(data)?)
    }
}
#[derive(Clone, Debug)]
pub struct OutputFormat {
    pub channels: RecorderChannelCount,
    pub sample_rate: RecorderSampleRate,
    pub sample_format: RecorderSampleFormat,
}

pub trait Recorder {
    fn output_format() -> OutputFormat;
    fn start(&mut self, output: ChannelRecorderOutput) -> RecorderResult<()>;
    fn stop(&mut self) -> RecorderResult<()>;
}

pub struct CpalRecorder {
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
}

impl Default for CpalRecorder {
    fn default() -> Self {
        CpalRecorder {
            input_stream: None,
            output_stream: None,
        }
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
    fn output_format() -> OutputFormat {
        OutputFormat {
            channels: 1,
            sample_rate: 48000,
            sample_format: cpal::SampleFormat::I16,
        }
    }
    fn start(&mut self, output: ChannelRecorderOutput) -> RecorderResult<()> {
        if self.input_stream.is_some() {
            return Err(RecorderError::Unknown);
        }
        let (device, config) = CpalRecorder::get_default_device()?;

        debug!(
            "Using device: {} config: {} channels, {} Hz, {:?}",
            device.name().unwrap_or_else(|_| "Unknown".to_string()),
            config.channels(),
            config.sample_rate().0,
            config.sample_format()
        );
        let output_config = device.default_output_config().unwrap();
        let output_stream = device.build_output_stream(
            &output_config.config(),
            move |data: &mut [f32], _| {
                for sample in data {
                    *sample = 0.0;
                }
            },
            |_| {},
            None,
        )?;
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
                    error!("Error sending data to output: {}", e);
                }
            },
            |err| {
                error!("Error occurred on input stream: {}", err);
            },
            None,
        )?;
        output_stream.play()?;
        stream.play()?;
        self.input_stream = Some(stream);
        self.output_stream = Some(output_stream);
        Ok(())
    }

    fn stop(&mut self) -> RecorderResult<()> {
        debug!("Stopping recorder...");
        if let Some(stream) = self.input_stream.take() {
            stream.pause()?;
            self.input_stream = None;
        }
        if let Some(output_stream) = self.output_stream.take() {
            output_stream.pause()?;
            self.output_stream = None;
        }
        Ok(())
    }
}
