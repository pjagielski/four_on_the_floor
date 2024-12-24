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
use midir::{MidiOutput, MidiOutputConnection};

use ctrlc;
mod midi;
mod model;
use model::Pattern;

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
            ("hh".to_string(), "samples/hh.wav".to_string()),
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


/// Plays a MIDI note using the provided MIDI connection.
fn play_midi_note(
    note: u8,
    velocity: f32,
    duration: f32,
    midi_conn: Arc<std::sync::Mutex<MidiOutputConnection>>,
) {
    let velocity = (velocity.max(0.0).min(127.0)) as u8;

    // MIDI Note On message
    if let Ok(mut conn) = midi_conn.lock() {
        let _ = conn.send(&[0x90, note, velocity]);
        println!("[MIDI] Note On: {}, velocity: {}, duration: {:.2}s", note, velocity, duration);
    }

    thread::sleep(Duration::from_secs_f32(duration));

    // MIDI Note Off message
    if let Ok(mut conn) = midi_conn.lock() {
        let _ = conn.send(&[0x80, note, 0]);
        println!("[MIDI] Note Off: {}", note);
    }
}

fn play_sound(
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

use threadpool::ThreadPool;

fn play_pattern_with_soundbank(
    patterns: Arc<Vec<Pattern>>,
    sound_bank: Arc<SoundBank>,
    stream_handle: Arc<OutputStreamHandle>,
    midi_conn: Arc<std::sync::Mutex<MidiOutputConnection>>,
    bpm: u32,
    loop_beats: u32,
) {
    let beat_duration = 60.0 / bpm as f32;
    let eighth_beat_duration = beat_duration / 8.0;
    let total_eighth_beats = loop_beats * 8;

    let start_time = Instant::now();
    let pool = ThreadPool::new(4); // Create a thread pool with 4 workers

    for i in 0..total_eighth_beats {
        let current_time_in_beats = i as f32 / 8.0;

        for pattern in patterns.iter() {
            if pattern.beats.contains(&current_time_in_beats) {
                let sb_clone = Arc::clone(&sound_bank);
                let sh_clone = Arc::clone(&stream_handle);
                let midi_conn_clone = Arc::clone(&midi_conn);
                let sound = pattern.sound.clone();
                let midi_note = pattern.midi_note;
                let velocity = pattern.velocity;
                let duration = pattern.duration;

                if let Some(note) = midi_note {
                    pool.execute(move || {
                        play_midi_note(note, velocity, duration, midi_conn_clone);
                    });
                }

                if let Some(label) = sound {
                    pool.execute(move || {
                        play_sound(&label, &sb_clone, &sh_clone, 100.0);
                    });
                }
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


fn generate_chord_patterns() -> Vec<Pattern> {
    let mut patterns = Vec::new();

    fn add_chord_pattern(
        patterns: &mut Vec<Pattern>,
        chord_notes: &[u8],
        chord_beats: &[f32],
        velocity: f32,
        duration: f32,
    ) {
        for &note in chord_notes {
            for &beat in chord_beats {
                patterns.push(Pattern {
                    sound: None,
                    midi_note: Some(note),
                    beats: vec![beat],
                    velocity,
                    duration,
                });
            }
        }
    }

    // Define chords and their beats
    let c_sharp_m = [61, 64, 68]; // C#4, E4, G#4
    let f_sharp_m = [66, 69, 73]; // F#4, A4, C#5
    let a_maj = [69, 73, 76];     // A4, C#5, E5
    let b_maj = [71, 75, 78];     // B4, D#5, F#5

    let c_sharp_beats = [0.25, 1.25, 1.75];
    let f_sharp_beats = [2.25, 3.25, 3.75];
    let a_beats       = [4.25, 5.25, 5.75];
    let b_beats       = [6.25, 6.50, 7.25, 7.75];

    add_chord_pattern(&mut patterns, &c_sharp_m, &c_sharp_beats, 100.0, 0.1);
    add_chord_pattern(&mut patterns, &f_sharp_m, &f_sharp_beats, 100.0, 0.1);
    add_chord_pattern(&mut patterns, &a_maj, &a_beats, 100.0, 0.1);
    add_chord_pattern(&mut patterns, &b_maj, &b_beats, 100.0, 0.1);

    patterns
}

fn repeat(beats: &[f32], size: usize, times: usize) -> Vec<f32> {
    // Initialize the result vector with the original beats
    let mut repeated_beats = beats.to_vec();

    // Loop to replicate beats `times` times
    for i in 1..times {
        let offset = size as f32 * i as f32; // Calculate the offset
        for &b in beats {
            repeated_beats.push(b + offset); // Add the offset to each beat
        }
    }

    repeated_beats
}

fn generate_combined_patterns(midi_pattern: Vec<Pattern>) -> Vec<Pattern> {
    let mut combined_patterns = Vec::new();

    // Add beat patterns
    combined_patterns.push(Pattern {
        sound: Some("bd".to_string()),
        beats: repeat(&vec![0.0, 0.75, 2.0, 2.75], 4, 2),
        midi_note: None,
        velocity: 100.0,
        duration: 0.25,
    });

    combined_patterns.push(Pattern {
        sound: Some("sd".to_string()),
        beats: repeat(&vec![1.5, 3.5], 4, 2),
        midi_note: None,
        velocity: 100.0,
        duration: 0.25,
    });

    // Create the Pattern
    combined_patterns.push(Pattern {
        sound: Some("hh".to_string()),
        beats: repeat(&vec![0.5, 1.25, 1.75], 2, 4),
        midi_note: None,
        velocity: 50.0,
        duration: 0.25,
    });

    // Add chord patterns
    combined_patterns.extend(generate_chord_patterns());

    combined_patterns.extend(midi_pattern);

    combined_patterns
}



/// -------------------------------------------------------------------------
/// 3) Main
/// -------------------------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up rodio
    let (_stream, stream_handle) = OutputStream::try_default()?;

    // Set up MIDI output
    let midi_out = MidiOutput::new("MIDI Output")?;
    let ports = midi_out.ports();
    let port = ports
        .iter()
        .find(|p| midi_out.port_name(p).map_or(false, |name| name == "IAC Driver Bus 1"))
        .ok_or("Could not find IAC Driver Bus 1 port")?;
    let conn = midi_out.connect(port, "IAC Driver Bus 1 Connection")?;
    let midi_conn = Arc::new(std::sync::Mutex::new(conn));

    // Wrap in Arc
    let sound_bank = Arc::new(SoundBank::new()?);
    let stream_handle = Arc::new(stream_handle);


    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <BPM>", args[0]);
        std::process::exit(1);
    }
    let bpm: u32 = args[1].parse()?;

    let loop_beats = 8;
    let midi_file = "shape.mid";
    let track_name = "Lead";

    let midi_pattern = midi::read_midi_and_extract_pattern(midi_file, track_name, bpm, 20.0);
    for pattern in &midi_pattern {
        println!("{:?}", pattern);
    }
    let patterns = Arc::new(generate_combined_patterns(midi_pattern));

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
            Arc::clone(&midi_conn),
            bpm,
            loop_beats,
        );
    }

    println!("All done. Exiting now...");
    Ok(())
}
