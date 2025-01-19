use midly::{Smf, TrackEventKind, MidiMessage};
use std::fs::File;
use std::io::Read;

use crate::model::Pattern;

use std::collections::HashMap;

pub fn read_midi_and_extract_pattern(
    file_path: &str,
    track_name: &str,
    bpm: u32,
    start_beat: f32,
    end_beat: f32,
) -> Vec<Pattern> {
    // Read the MIDI file into memory
    let mut file = File::open(file_path).expect("Failed to open MIDI file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Failed to read MIDI file");

    // Parse the MIDI file
    let smf = Smf::parse(&buffer).expect("Failed to parse MIDI file");

    // Time conversion constants
    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(tpb) => tpb.as_int() as f32,
        _ => panic!("Unsupported MIDI timing format"),
    };
    let seconds_per_tick = 60.0 / (bpm as f32 * ticks_per_beat);
    let increment = 0.25; // Round to nearest 0.25

    // Initialize patterns and active notes
    let mut patterns = Vec::new();
    let mut active_notes: HashMap<u8, (f32, f32)> = HashMap::new();

    // Define an anonymous function (closure) for common logic
    let mut handle_note_off = |key: u8, current_seconds: f32, active_notes: &mut HashMap<u8, (f32, f32)>| {
        if let Some((start_time, velocity)) = active_notes.remove(&key) {
            let duration = current_seconds - start_time;
            let beat_start = start_time / (60.0 / bpm as f32);

            // Round to nearest increment
            let rounded_beat_start = (beat_start / increment).round() * increment;

            // Filter patterns within the specified beat range
            if rounded_beat_start >= start_beat && rounded_beat_start < end_beat {
                patterns.push(Pattern {
                    sound: None,
                    loop_name: None,
                    midi_note: Some(key),
                    beats: vec![rounded_beat_start - start_beat],
                    velocity: velocity / 127.0 * 100.0,
                    duration,
                });
            }
        }
    };

    // Process each track
    for track in smf.tracks.iter() {
        let mut found_name = false;

        // Check if this is the desired track
        for event in track.iter() {
            if let TrackEventKind::Meta(midly::MetaMessage::TrackName(name)) = &event.kind {
                let track_name_bytes: Vec<u8> = name.iter().cloned().collect();
                if let Ok(name_str) = String::from_utf8(track_name_bytes) {
                    println!("Track {}", name_str);
                    if name_str == track_name {
                        found_name = true;
                        break;
                    }
                }
            }
        }

        if !found_name {
            continue;
        }

        // Process events in the track
        let mut current_time: u32 = 0;
        for event in track.iter() {
            current_time += event.delta.as_int();

            let current_seconds = current_time as f32 * seconds_per_tick;

            match &event.kind {
                // Handle Note On events with velocity > 0
                TrackEventKind::Midi {
                    message: MidiMessage::NoteOn { key, vel },
                    ..
                } if vel.as_int() > 0 => {
                    active_notes.insert(key.as_int(), (current_seconds, vel.as_int() as f32));
                }

                // Common logic for NoteOff and NoteOn with vel = 0
                TrackEventKind::Midi {
                    message: MidiMessage::NoteOff { key, vel: _ },
                    ..
                } => {
                    handle_note_off(key.as_int(), current_seconds, &mut active_notes);
                }
                | TrackEventKind::Midi {
                    message: MidiMessage::NoteOn { key, vel },
                    ..
                } if vel.as_int() == 0 => {
                    handle_note_off(key.as_int(), current_seconds, &mut active_notes);
                }

                _ => {}
            }
        }
    }

    patterns
}

