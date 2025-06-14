use cpal::Sample;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::{debug, error};
use std::time::SystemTime;
use thiserror::Error;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

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

#[derive(Clone, Debug)]
pub struct OutputFormat {
    pub channels: RecorderChannelCount,
    pub sample_rate: RecorderSampleRate,
    pub sample_format: RecorderSampleFormat,
}

pub struct SampleData {
    pub data: Vec<i16>,
    pub timestamp: u64,
}

pub struct Started {
    input_stream: cpal::Stream,
    output_stream: cpal::Stream,
    sample_data_receiver: UnboundedReceiver<SampleData>,
}

pub struct Stopped;

pub struct CpalRecorder<State = Stopped> {
    state: State,
}

impl Default for CpalRecorder {
    fn default() -> Self {
        CpalRecorder { state: Stopped }
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
    pub fn output_format() -> OutputFormat {
        OutputFormat {
            channels: 1,
            sample_rate: 48000,
            sample_format: cpal::SampleFormat::I16,
        }
    }
}

impl CpalRecorder<Stopped> {
    pub fn start(self) -> RecorderResult<CpalRecorder<Started>> {
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
        let (tx, rx) = unbounded_channel();
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
                let raw_sample_data = data
                    .iter()
                    .map(|&s| {
                        return i16::from_sample(s.clone());
                    })
                    .collect::<Vec<i16>>();
                let sample_data = SampleData {
                    data: raw_sample_data,
                    timestamp: SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                };
                tx.send(sample_data)
                    .expect("Failed to send data to channel");
            },
            |err| {
                error!("Error occurred on input stream: {}", err);
            },
            None,
        )?;
        output_stream.play()?;
        stream.play()?;
        let state = Started {
            input_stream: stream,
            output_stream: output_stream,
            sample_data_receiver: rx,
        };
        Ok(CpalRecorder { state })
    }
}

impl CpalRecorder<Started> {
    pub async fn reveice_sample_data(&mut self) -> Option<SampleData> {
        return self.state.sample_data_receiver.recv().await;
    }

    pub fn stop(self) -> RecorderResult<CpalRecorder<Stopped>> {
        debug!("Stopping recorder...");
        self.state.input_stream.pause()?;
        self.state.output_stream.pause()?;
        Ok(CpalRecorder { state: Stopped })
    }
}
