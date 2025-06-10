use std::fs::File;
use std::io::BufWriter;

use cpal::{FromSample, Sample};
use hound::WavWriter;

use crate::recorder::OutputFormat;

fn sample_format(format: cpal::SampleFormat) -> hound::SampleFormat {
    if format.is_float() {
        hound::SampleFormat::Float
    } else {
        hound::SampleFormat::Int
    }
}

fn wav_spec_from_config(config: &OutputFormat) -> hound::WavSpec {
    hound::WavSpec {
        channels: config.channels,
        sample_rate: config.sample_rate,
        bits_per_sample: (config.sample_format.sample_size() * 8) as _,
        sample_format: sample_format(config.sample_format),
    }
}

pub struct Wav {
    writer: WavWriter<BufWriter<File>>,
}

impl Wav {
    pub fn new(path: &str, config: &OutputFormat) -> Self {
        let wav_spec = wav_spec_from_config(config);
        let file = File::create(path).expect("Failed to create WAV file");
        let file = BufWriter::new(file);
        let writer = hound::WavWriter::new(file, wav_spec).expect("Failed to create WAV writer");

        Wav { writer: writer }
    }

    pub fn write<T, U>(&mut self, input: &[T]) -> hound::Result<()>
    where
        T: Sample,
        U: Sample + hound::Sample + FromSample<T>,
    {
        for &sample in input.iter() {
            let sample: U = U::from_sample(sample);
            self.writer.write_sample(sample).ok();
        }
        Ok(())
    }

    pub fn save(self) -> hound::Result<()> {
        self.writer.finalize().unwrap();
        Ok(())
    }
}
