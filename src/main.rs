use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use std::env;

use ctrlc;

/// -------------------------------------------------------------------------
/// 1) SoundBank
/// -------------------------------------------------------------------------
struct SoundBank {
    data: HashMap<String, (Vec<i16>, u16, u32)>,
}

fn load_sample(path: &str) -> Result<(Vec<i16>, u16, u32), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let decoder = Decoder::new(BufReader::new(file))?;
    // We need the Source trait in scope for channels() & sample_rate().
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<i16> = decoder.convert_samples().collect();
    Ok((samples, channels, sample_rate))
}

impl SoundBank {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut data = HashMap::new();

        // Label -> WAV path
        let sound_paths = [
            ("bd".to_string(), "samples/bd.wav".to_string()),
            ("sd".to_string(), "samples/sd.wav".to_string()),
        ];

        for (label, path) in sound_paths {
            let (samples, channels, rate) = load_sample(&path)?;
            data.insert(label, (samples, channels, rate));
        }

        Ok(SoundBank { data })
    }

    fn get(&self, label: &str) -> Option<&(Vec<i16>, u16, u32)> {
        self.data.get(label)
    }
}

/// -------------------------------------------------------------------------
/// 2) Pattern + Playback
/// -------------------------------------------------------------------------
#[derive(Debug)]
pub struct Pattern {
    pub sound: Option<String>,
    pub midi_note: Option<u8>,
    pub beats: Vec<f32>,
    pub velocity: f32,
    pub duration: f32,
}

/// Stub for MIDI (unused in this example).
fn play_midi_note(note: u8, velocity: f32, duration: f32) {
    println!(
        "[MIDI] Note = {}, Velocity = {:.1}, Duration = {:.2}s (stub)",
        note, velocity, duration
    );
}

fn play_sound_label(
    label: &str,
    sound_bank: &SoundBank,
    stream_handle: &OutputStreamHandle,
    velocity: f32,
) {
    if let Some((samples, channels, sample_rate)) = sound_bank.get(label) {
        let sink = Sink::try_new(stream_handle).unwrap();
        let source =
            rodio::buffer::SamplesBuffer::new(*channels, *sample_rate, samples.clone());
        sink.append(source);
        sink.detach();
        println!("[Audio] Playing '{}' at velocity {:.1}", label, velocity);
    } else {
        println!("Warning: No sound label '{}' found in SoundBank", label);
    }
}

/// Schedules patterns over a certain number of beats (like your original approach).
fn play_pattern_with_soundbank(
    patterns: Arc<Vec<Pattern>>,
    sound_bank: Arc<SoundBank>,
    stream_handle: Arc<OutputStreamHandle>,
    bpm: u32,
    loop_beats: u32,
) {
    let beat_duration = 60.0 / bpm as f32;
    let eighth_beat_duration = beat_duration / 8.0;
    let total_eighth_beats = loop_beats * 8;

    let start_time = Instant::now();

    for i in 0..total_eighth_beats {
        let current_time_in_beats = i as f32 / 8.0;

        for pattern in patterns.iter() {
            if pattern.beats.contains(&current_time_in_beats) {
                let sb_clone = Arc::clone(&sound_bank);
                let sh_clone = Arc::clone(&stream_handle);
                let sound = pattern.sound.clone();

                thread::spawn(move || {
                    if let Some(label) = sound {
                        play_sound_label(&label, &sb_clone, &sh_clone, 100.0);
                    }
                });
            }
        }

        let elapsed = start_time.elapsed().as_secs_f32();
        let target_time = (i + 1) as f32 * eighth_beat_duration;
        let remaining = target_time - elapsed;
        if remaining > 0.0 {
            thread::sleep(Duration::from_secs_f32(remaining));
        }
    }
}



/// -------------------------------------------------------------------------
/// 3) Main
/// -------------------------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up rodio
    let (_stream, stream_handle) = OutputStream::try_default()?;

    // Wrap in Arc
    let sound_bank = Arc::new(SoundBank::new()?);
    let stream_handle = Arc::new(stream_handle);

    // Example patterns
    let patterns = vec![
        Pattern {
            sound: Some("bd".to_string()),
            beats: vec![0.0, 0.75, 2.0, 2.75],
            midi_note: None,
            velocity: 100.0,
            duration: 0.25,
        },
        Pattern {
            sound: Some("sd".to_string()),
            beats: vec![1.5, 3.5],
            midi_note: None,
            velocity: 100.0,
            duration: 0.25,
        },
    ];

    let patterns = Arc::new(patterns);

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <BPM>", args[0]);
        std::process::exit(1);
    }
    let bpm: u32 = args[1].parse()?;

    let loop_beats = 4;

    // We'll keep looping until Ctrl+C
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Set up Ctrl+C handler
    ctrlc::set_handler(move || {
        println!("Ctrl+C detected. Stopping loop...");
        r.store(false, Ordering::SeqCst);
    })?;

    println!("Press Ctrl+C to stop the loop.");

    while running.load(Ordering::SeqCst) {
        play_pattern_with_soundbank(
            Arc::clone(&patterns),
            Arc::clone(&sound_bank),
            Arc::clone(&stream_handle),
            bpm,
            loop_beats,
        );
    }

    println!("All done. Exiting now...");
    Ok(())
}
