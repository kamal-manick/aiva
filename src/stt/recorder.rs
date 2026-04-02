use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, Stream};
use hound::{WavSpec, WavWriter};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<Stream>,
    sample_rate: u32,
    is_recording: bool,
}

impl AudioRecorder {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            sample_rate: 16000, // Whisper expects 16kHz
            is_recording: false,
        })
    }

    pub fn start_recording(&mut self) -> Result<(), String> {
        if self.is_recording {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        println!("Using input device: {}", device.name().unwrap_or_default());

        // Collect all supported configs first
        let supported_configs: Vec<_> = device
            .supported_input_configs()
            .map_err(|e| format!("Failed to get input configs: {}", e))?
            .collect();

        if supported_configs.is_empty() {
            return Err("No input configurations available on device".to_string());
        }

        // Define priority order for sample formats (best to worst)
        let format_priority = [
            SampleFormat::F32,  // Best: no conversion needed
            SampleFormat::I16,  // Already supported
            SampleFormat::U16,  // Common on Windows devices
            SampleFormat::I32,  // Less common
            SampleFormat::U8,   // Fallback for basic devices
        ];

        // Try each format in priority order
        let mut config_option = None;
        for &preferred_format in &format_priority {
            // Debug: show configs for this format
            let matching_by_format: Vec<_> = supported_configs
                .iter()
                .filter(|c| c.sample_format() == preferred_format)
                .collect();
            if !matching_by_format.is_empty() {
                println!("  Found {} configs with format {:?}", matching_by_format.len(), preferred_format);
                for cfg in &matching_by_format {
                    println!("    - {}ch, {}-{}Hz", cfg.channels(), cfg.min_sample_rate().0, cfg.max_sample_rate().0);
                }
            }

            config_option = supported_configs
                .iter()
                .filter(|c| c.channels() >= 1 && c.channels() <= 4)  // Accept 1-4 channels (microphone arrays)
                .filter(|c| c.sample_format() == preferred_format)
                .min_by_key(|c| {
                    // Prefer configs closer to 16kHz (Whisper STT requirement)
                    let min_rate = c.min_sample_rate().0;
                    let max_rate = c.max_sample_rate().0;
                    if min_rate <= 16000 && max_rate >= 16000 {
                        0
                    } else {
                        (min_rate as i32 - 16000).abs().min((max_rate as i32 - 16000).abs())
                    }
                })
                .cloned();

            if config_option.is_some() {
                println!("Selected audio format: {:?}", preferred_format);
                break;
            }
        }

        // If no suitable config found, provide detailed diagnostics
        let config = config_option.ok_or_else(|| {
            let available_formats: Vec<String> = supported_configs
                .iter()
                .map(|c| format!(
                    "{:?} ({}ch, {}-{}Hz)",
                    c.sample_format(),
                    c.channels(),
                    c.min_sample_rate().0,
                    c.max_sample_rate().0
                ))
                .collect();

            format!(
                "No suitable input config found.\n\
                 Required: 1-4 channels\n\
                 Available formats on device:\n  {}",
                available_formats.join("\n  ")
            )
        })?;

        let sample_rate = if config.min_sample_rate().0 <= 16000 && config.max_sample_rate().0 >= 16000 {
            16000
        } else {
            config.min_sample_rate().0.max(config.max_sample_rate().0.min(44100))
        };

        let config = config.with_sample_rate(SampleRate(sample_rate));
        let sample_format = config.sample_format();
        let channels = config.channels();

        self.sample_rate = sample_rate;
        self.samples.lock().unwrap().clear();

        let samples = self.samples.clone();

        let err_fn = |err| eprintln!("Audio stream error: {}", err);

        let stream = match sample_format {
            SampleFormat::F32 => {
                let config = config.into();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[f32], _: &_| {
                            let mut samples = samples.lock().unwrap();
                            // Convert to mono by averaging channels
                            if channels == 1 {
                                samples.extend_from_slice(data);
                            } else {
                                for chunk in data.chunks(channels as usize) {
                                    let sum: f32 = chunk.iter().sum();
                                    samples.push(sum / channels as f32);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| format!("Failed to build input stream: {}", e))?
            }
            SampleFormat::I16 => {
                let config = config.into();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i16], _: &_| {
                            let mut samples = samples.lock().unwrap();
                            // Convert to mono by averaging channels, and to f32
                            if channels == 1 {
                                for &sample in data {
                                    samples.push(sample as f32 / 32768.0);
                                }
                            } else {
                                for chunk in data.chunks(channels as usize) {
                                    let sum: f32 = chunk.iter().map(|&s| s as f32).sum();
                                    samples.push(sum / 32768.0 / channels as f32);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| format!("Failed to build input stream: {}", e))?
            }
            SampleFormat::U16 => {
                let config = config.into();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[u16], _: &_| {
                            let mut samples = samples.lock().unwrap();
                            // Convert to mono by averaging channels, and to f32
                            if channels == 1 {
                                for &sample in data {
                                    samples.push(sample_conversion::u16_to_f32(sample));
                                }
                            } else {
                                for chunk in data.chunks(channels as usize) {
                                    let sum: f32 = chunk.iter().map(|&s| sample_conversion::u16_to_f32(s)).sum();
                                    samples.push(sum / channels as f32);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| format!("Failed to build input stream: {}", e))?
            }
            SampleFormat::U8 => {
                let config = config.into();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[u8], _: &_| {
                            let mut samples = samples.lock().unwrap();
                            // Convert to mono by averaging channels, and to f32
                            if channels == 1 {
                                for &sample in data {
                                    samples.push(sample_conversion::u8_to_f32(sample));
                                }
                            } else {
                                for chunk in data.chunks(channels as usize) {
                                    let sum: f32 = chunk.iter().map(|&s| sample_conversion::u8_to_f32(s)).sum();
                                    samples.push(sum / channels as f32);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| format!("Failed to build input stream: {}", e))?
            }
            SampleFormat::I32 => {
                let config = config.into();
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i32], _: &_| {
                            let mut samples = samples.lock().unwrap();
                            // Convert to mono by averaging channels, and to f32
                            if channels == 1 {
                                for &sample in data {
                                    samples.push(sample_conversion::i32_to_f32(sample));
                                }
                            } else {
                                for chunk in data.chunks(channels as usize) {
                                    let sum: f32 = chunk.iter().map(|&s| sample_conversion::i32_to_f32(s)).sum();
                                    samples.push(sum / channels as f32);
                                }
                            }
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| format!("Failed to build input stream: {}", e))?
            }
            _ => return Err(format!("Unsupported sample format: {:?}", sample_format)),
        };

        stream.play().map_err(|e| format!("Failed to start stream: {}", e))?;

        self.stream = Some(stream);
        self.is_recording = true;

        println!("Recording started at {}Hz", sample_rate);
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<Vec<u8>, String> {
        if !self.is_recording {
            return Err("Not recording".to_string());
        }

        // Drop the stream to stop recording
        self.stream = None;
        self.is_recording = false;

        let samples = self.samples.lock().unwrap().clone();
        println!("Recording stopped. {} samples captured", samples.len());

        if samples.is_empty() {
            return Err("No audio captured".to_string());
        }

        // Resample to 16kHz if needed
        let samples = if self.sample_rate != 16000 {
            Self::resample(&samples, self.sample_rate, 16000)
        } else {
            samples
        };

        // Encode as WAV
        self.encode_wav(&samples)
    }

    fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate {
            return samples.to_vec();
        }

        let ratio = from_rate as f64 / to_rate as f64;
        let new_len = (samples.len() as f64 / ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = i as f64 * ratio;
            let idx0 = src_idx as usize;
            let idx1 = (idx0 + 1).min(samples.len() - 1);
            let frac = src_idx - idx0 as f64;

            // Linear interpolation
            let sample = samples[idx0] as f64 * (1.0 - frac) + samples[idx1] as f64 * frac;
            resampled.push(sample as f32);
        }

        resampled
    }

    fn encode_wav(&self, samples: &[f32]) -> Result<Vec<u8>, String> {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = WavWriter::new(&mut buffer, spec)
                .map_err(|e| format!("Failed to create WAV writer: {}", e))?;

            for &sample in samples {
                // Convert f32 [-1.0, 1.0] to i16
                let sample_i16 = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
                writer
                    .write_sample(sample_i16)
                    .map_err(|e| format!("Failed to write sample: {}", e))?;
            }

            writer
                .finalize()
                .map_err(|e| format!("Failed to finalize WAV: {}", e))?;
        }

        Ok(buffer.into_inner())
    }
}

/// Audio sample format conversion utilities
mod sample_conversion {
    /// Convert U16 sample to F32 in range [-1.0, 1.0]
    pub fn u16_to_f32(sample: u16) -> f32 {
        (sample as f32 - 32768.0) / 32768.0
    }

    /// Convert U8 sample to F32 in range [-1.0, 1.0]
    pub fn u8_to_f32(sample: u8) -> f32 {
        (sample as f32 - 128.0) / 128.0
    }

    /// Convert I32 sample to F32 in range [-1.0, 1.0]
    pub fn i32_to_f32(sample: i32) -> f32 {
        sample as f32 / 2147483648.0
    }

    /// Convert stereo to mono by averaging
    pub fn stereo_to_mono_f32(left: f32, right: f32) -> f32 {
        (left + right) / 2.0
    }
}
