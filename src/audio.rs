use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use anyhow::{Result, Context};
use std::sync::mpsc::Sender;

pub fn start_audio_capture(tx: Sender<Vec<i16>>) -> Result<(u32, u16)> {
    let host = cpal::default_host();
    
    // Simplest approach: Use default output but treat it as input
    // If this fails on some Windows setups, we fall back to searching for ANY input device
    let device = host.default_output_device()
        .or_else(|| host.default_input_device())
        .context("No se encontró ningún dispositivo de audio")?;

    println!("🔊 Capturando desde: {}", device.name().unwrap_or_default());

    let config = device.default_output_config()
        .or_else(|_| device.default_input_config())
        .context("No se pudo obtener la configuración de audio")?;
    
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();
    
    let stream_config: cpal::StreamConfig = config.clone().into();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let pcm: Vec<i16> = data.iter()
                        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                        .collect();
                    let _ = tx.send(pcm);
                },
                |err| eprintln!("Audio error: {}", err),
                None
            )?
        },
        cpal::SampleFormat::I16 => {
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let _ = tx.send(data.to_vec());
                },
                |err| eprintln!("Audio error: {}", err),
                None
            )?
        },
        _ => anyhow::bail!("Unsupported format"),
    };

    stream.play()?;
    std::mem::forget(stream);
    
    Ok((sample_rate, channels))
}
