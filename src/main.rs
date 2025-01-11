use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::{
    fs,
    sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}},
    thread,
    time::{Duration, Instant},
};
use std::env;
use midir::{MidiOutput, MidiOutputConnection};

use ctrlc;
mod midi;
mod model;
mod config;

use model::{Pattern, PatternBuilder};
use config::Config;


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
    fn new(directory: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut data = HashMap::new();

        // Read all files in the given directory using a thread pool
        let paths = fs::read_dir(directory)?;
        let pool = ThreadPool::new(4);
        let results = Arc::new(std::sync::Mutex::new(Vec::new()));

        for path in paths {
            let path = path?.path();
            if let Some(extension) = path.extension() {
                if extension == "wav" {
                    let path_str = path.to_str().ok_or("Invalid file path")?.to_string();
                    let results_clone = Arc::clone(&results);

                    pool.execute(move || {
                        println!("Loading {}", path_str);
                        match load_sample(&path_str) {
                            Ok((samples, channels, rate)) => {
                                let label = std::path::Path::new(&path_str)
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or_default()
                                    .to_string();
                                results_clone.lock().unwrap().push((label, (samples, channels, rate)));
                            }
                            Err(e) => {
                                eprintln!("Failed to load sample '{}': {}", path_str, e);
                            }
                        }
                    });
                }
            }
        }

        // Wait for all threads to finish
        pool.join();

        // Collect results into the data map
        for (label, data_entry) in results.lock().unwrap().drain(..) {
            data.insert(label, data_entry);
        }

        Ok(SoundBank { data })
    }

    fn get(&self, label: &str) -> Option<&(Vec<i16>, u16, u32)> {
        self.data.get(label)
    }
}


struct LoopBank {
    data: HashMap<String, (Vec<i16>, u16, u32, u32)>, // (samples, channels, sample_rate, beats)
}

fn load_loop(path: &str) -> Result<(Vec<i16>, u16, u32, u32, String), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let decoder = Decoder::new(BufReader::new(file))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<i16> = decoder.convert_samples().collect();

    // Extract bpm and beats from filename
    let filename = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Invalid filename")?;

    let parts: Vec<&str> = filename.split('_').collect();
    if parts.len() != 3 {
        return Err("Invalid loop filename format. Expected: bpm_beats_name.wav".into());
    }

    let bpm: u32 = parts[0].parse()?;
    let name: &str = parts[2];

    Ok((samples, channels, sample_rate, bpm, name.to_string()))
}


impl LoopBank {
    fn new(directory: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut data = HashMap::new();

        // Read all files in the given directory using a thread pool
        let paths = fs::read_dir(directory)?;
        let pool = ThreadPool::new(16);
        let results = Arc::new(std::sync::Mutex::new(Vec::new()));

        for path in paths {
            let path = path?.path();
            if let Some(extension) = path.extension() {
                if extension == "wav" {
                    let path_str = path.to_str().ok_or("Invalid file path")?.to_string();
                    let results_clone = Arc::clone(&results);

                    pool.execute(move || {
                        println!("Loading {}", path_str);
                        match load_loop(&path_str) {
                            Ok((samples, channels, rate, total_beats, name)) => {
                                results_clone.lock().unwrap().push((name, (samples, channels, rate, total_beats)));
                            }
                            Err(e) => {
                                eprintln!("Failed to load loop '{}': {}", path_str, e);
                            }
                        }
                    });
                }
            }
        }

        // Wait for all threads to finish
        pool.join();

        // Collect results into the data map
        for (label, data_entry) in results.lock().unwrap().drain(..) {
            data.insert(label, data_entry);
        }

        Ok(LoopBank { data })
    }

    fn get(&self, label: &str) -> Option<&(Vec<i16>, u16, u32, u32)> {
        self.data.get(label)
    }
}

fn beats_to_millis(beats: f32, bpm: u32) -> u64 {
    let minutes = beats / bpm as f32;
    let millis = minutes * 60.0 * 1000.0;
    millis.round() as u64
}

fn play_loop(
    label: &str,
    duration: f32,
    velocity: f32,
    loop_bank: &LoopBank,
    stream_handle: &OutputStreamHandle,
    project_bpm: u32,
) {
    if let Some((samples, channels, sample_rate, loop_bpm_beats)) = loop_bank.get(label) {
        let original_bpm = *loop_bpm_beats;
        let playback_speed = project_bpm as f32 / original_bpm as f32;
        let duration_millis = beats_to_millis(duration, project_bpm);

        let source = rodio::buffer::SamplesBuffer::new(*channels, *sample_rate, samples.to_vec())
            .buffered()
            .amplify(velocity / 100.0)
            // .reverb(Duration::from_millis(delay as u64), 0.8) // Add delay for reverb effect
            .take_duration(Duration::from_millis(duration_millis))
            .speed(playback_speed); // Adjust speed for BPM
        let sink = Sink::try_new(stream_handle).unwrap();
        sink.append(source);
        sink.detach();
        println!(
            "[Loop] Playing '{}' at project BPM {} for original {} with speed adjustment {:.2}",
            label, project_bpm, original_bpm, playback_speed
        );
    } else {
        println!("Warning: No loop label '{}' found in LoopBank", label);
    }
}




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
    velocity: f32,
    sound_bank: &SoundBank,
    stream_handle: &OutputStreamHandle,
) {
    if let Some((samples, channels, sample_rate)) = sound_bank.get(label) {
        let sink = Sink::try_new(stream_handle).unwrap();
        let source =
            rodio::buffer::SamplesBuffer::new(*channels, *sample_rate, samples.clone())
            .amplify(velocity / 100.0);
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
    current_beat: Arc<RwLock<f32>>,
    sound_bank: Arc<SoundBank>,
    loop_bank: Arc<LoopBank>,
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
        let computed_current_beat = i as f32 / 8.0;
        {
            let mut beat_lock = current_beat.write().unwrap();
            *beat_lock = computed_current_beat;
        }

        for pattern in patterns.iter() {
            if pattern.beats.contains(&computed_current_beat) {
                let sb_clone = Arc::clone(&sound_bank);
                let sh_clone = Arc::clone(&stream_handle);
                let midi_conn_clone = Arc::clone(&midi_conn);
                let sound = pattern.sound.clone();
                let loop_name = pattern.loop_name.clone();
                let midi_note = pattern.midi_note;
                let velocity = pattern.velocity;
                let duration = pattern.duration;

                if let Some(note) = midi_note {
                    pool.execute(move || {
                        play_midi_note(note, velocity, duration, midi_conn_clone);
                    });
                }

                else if let Some(label) = sound {
                    pool.execute(move || {
                        play_sound(&label,  velocity, &sb_clone, &sh_clone);
                    });
                }

                else if let Some(loop_name) = loop_name {
                    let lb_clone = Arc::clone(&loop_bank);
                    pool.execute(move || {
                        play_loop(&loop_name, duration, velocity, &lb_clone, &sh_clone, bpm);
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
                    loop_name: None,
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

fn generate_combined_patterns(midi_pattern: Vec<Pattern>, json_patterns: Vec<Pattern>) -> Vec<Pattern> {
    let mut combined_patterns = Vec::new();

    combined_patterns.extend(json_patterns);

    combined_patterns.push(PatternBuilder::new()
        .loop_name("dl-ethnic")
        .beats(vec![0.0, 4.0])
        .duration(2.0)
        .build()
    );
    combined_patterns.push(PatternBuilder::new()
        .loop_name("dl-ethnic")
        .beats(vec![1.25, 5.25])
        .duration(2.5)
        .build()
    );

    // // Add chord patterns
    combined_patterns.extend(generate_chord_patterns());

    combined_patterns.extend(midi_pattern);

    combined_patterns
}

use eframe::egui;

pub struct PatternVisualizerApp {
    patterns: Arc<RwLock<Vec<Pattern>>>,
    current_beat: Arc<RwLock<f32>>,
    gui_ready: Arc<AtomicBool>,
    bpm: u32,
}

impl PatternVisualizerApp {
    pub fn new(
        patterns: Arc<RwLock<Vec<Pattern>>>,
        current_beat: Arc<RwLock<f32>>,
        gui_ready: Arc<AtomicBool>,
        bpm: u32,
    ) -> Self {
        Self {
            patterns,
            current_beat,
            gui_ready,
            bpm,
        }
    }

    pub fn update_grid(&self) -> f32 {
        let current_beat = self.current_beat.read().unwrap();
        *current_beat
    }
}


impl eframe::App for PatternVisualizerApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let loop_beats = 8;
        let resolution = 0.25;
        let total_eighth_beats = (loop_beats as f32 / resolution) as i32;
        let current_beat = self.update_grid();

        let beat_duration = 60.0 / self.bpm as f32;
        let delay_time = Duration::from_secs_f32((beat_duration * resolution) - 0.15);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Rust 4x4 Groovebox");
                let spacing = ui.spacing_mut();
                spacing.item_spacing = egui::vec2(5.0, 5.0); // No spacing between items

                let cell_size = 20.0;

                let sample_patterns: Vec<_> = {
                    let patterns_lock = self.patterns.read().unwrap();
                    patterns_lock
                        .iter()
                        .filter(|pattern| pattern.sound.is_some()) // Example: Filter non-empty sound
                        .cloned()
                        .collect()
                };

                let grid_width = 50.0 + total_eighth_beats as f32 * (cell_size + 5.0);
                let grid_height = 100.0 + sample_patterns.len() as f32 * (cell_size + 5.0);
        
                // Adjust the window size to fit the grid
                frame.set_window_size(egui::vec2(grid_width, grid_height));

                for pattern in sample_patterns.iter() {
                    ui.horizontal(|ui| {
                        for col_index in 0..total_eighth_beats {
                            let beat = col_index as f32 * resolution;
                            let is_active = pattern.beats.contains(&beat);
                            let is_playing = current_beat == beat; // Highlight current beat

                            let color = if is_playing && is_active {
                                egui::Color32::YELLOW
                            } else if is_active {
                                egui::Color32::RED
                            } else {
                                egui::Color32::WHITE
                            };

                            egui::Frame::default()
                                .fill(color)
                                .stroke(egui::Stroke::new(1.0, egui::Color32::BLACK))
                                .show(ui, |ui| {
                                    ui.allocate_space(egui::vec2(cell_size, cell_size));
                                });
                        }
                    });
                }
            });
        });
        self.gui_ready.store(true, Ordering::SeqCst);
        std::thread::sleep(delay_time);
        ctx.request_repaint(); // Ensure continuous UI updates
    }
}



fn load_and_combine_patterns(file_path: &str, midi_pattern: &Vec<Pattern>) -> Vec<Pattern> {
    if let Ok(file_content) = fs::read_to_string(file_path) {
        load_and_combine_patterns_from_content(&file_content, midi_pattern)
    } else {
        eprintln!("Failed to read {} during initial load.", file_path);
        generate_combined_patterns(midi_pattern.clone(), Vec::new())
    }
}

/// Helper function to load and combine patterns from file content
fn load_and_combine_patterns_from_content(
    file_content: &str,
    midi_pattern: &Vec<Pattern>,
) -> Vec<Pattern> {
    match serde_json::from_str::<Vec<Pattern>>(file_content) {
        Ok(new_patterns) => generate_combined_patterns(midi_pattern.clone(), new_patterns),
        Err(e) => {
            eprintln!("Failed to parse JSON: {}", e);
            generate_combined_patterns(midi_pattern.clone(), Vec::new())
        }
    }
}

/// -------------------------------------------------------------------------
/// 3) Main
/// -------------------------------------------------------------------------
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read config
    let config = config::read_config("config.json")?;

    // Set up rodio
    let (_stream, stream_handle) = OutputStream::try_default()?;

    // Set up MIDI output
    let midi_out = MidiOutput::new("MIDI Output")?;
    let ports = midi_out.ports();
    let port = ports
        .iter()
        .find(|p| midi_out.port_name(p).map_or(false, |name| name == config.midi_port))
        .ok_or(format!("Could not find {} port", config.midi_port))?;
    let conn = midi_out.connect(port, &config.midi_port)?;
    let midi_conn = Arc::new(std::sync::Mutex::new(conn));

    // Wrap in Arc
    let sound_bank: Arc<SoundBank> = Arc::new(SoundBank::new(&config.sounds.samples)?);
    let stream_handle = Arc::new(stream_handle);
    let loop_bank = Arc::new(LoopBank::new(&config.sounds.loops)?);

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <BPM> [--no-gui]", args[0]);
        std::process::exit(1);
    }
    let bpm: u32 = args[1].parse()?;
    let show_gui = !args.contains(&"--no-gui".to_string());

    let loop_beats = 8;
    let midi_pattern = midi::read_midi_and_extract_pattern(
        &config.midi_track.midi_file,
        &config.midi_track.track_name,
        bpm,
        config.midi_track.limit_beats,
    );
    
    // Atomic flag for stopping threads
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    // Set up Ctrl+C handler
    ctrlc::set_handler(move || {
        println!("Ctrl+C detected. Stopping loop...");
        r.store(false, Ordering::SeqCst);
    })?;
    println!("Press Ctrl+C to stop the loop.");

    // Shared state for the patterns
    let patterns = Arc::new(RwLock::new(Vec::new()));

    {
        let initial_patterns = load_and_combine_patterns("patterns.json", &midi_pattern);
        let mut patterns_write = patterns.write().unwrap();
        *patterns_write = initial_patterns;
    }

    // Start a background thread to watch for changes
    let patterns_clone = Arc::clone(&patterns);
    let running_clone = Arc::clone(&running);
    let midi_pattern_clone = midi_pattern.clone(); // Clone MIDI patterns for the thread
    thread::spawn(move || {
        loop {
            if running_clone.load(Ordering::SeqCst) {
                if let Ok(file_content) = fs::read_to_string("patterns.json") {
                    let combined_patterns = load_and_combine_patterns_from_content(
                        &file_content,
                        &midi_pattern_clone,
                    );
                    let mut patterns_write = patterns_clone.write().unwrap(); // Write lock
                    *patterns_write = combined_patterns;
                    println!("Patterns updated from JSON and MIDI patterns combined.");
                } else {
                    eprintln!("Failed to read patterns.json");
                }
            } else {
                break;
            }
            thread::sleep(Duration::from_secs(3));
        }
    });

    let current_beat = Arc::new(RwLock::new(0.0)); // Shared state for the current beat
    let gui_current_beat = Arc::clone(&current_beat);
    let gui_patterns = Arc::clone(&patterns);
    let gui_ready = Arc::new(AtomicBool::new(false)); // Flag to signal when GUI is ready
    let playback_gui_ready = Arc::clone(&gui_ready);

    let playback_handle = std::thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            // Load the current patterns
            let current_patterns = {
                let patterns_lock = patterns.read().unwrap();
                patterns_lock.clone()
            };

            while !playback_gui_ready.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            println!("Starting playback");

            // Play the pattern with the sound bank
            play_pattern_with_soundbank(
                Arc::new(current_patterns),
                Arc::clone(&current_beat),
                Arc::clone(&sound_bank),
                Arc::clone(&loop_bank),
                Arc::clone(&stream_handle),
                Arc::clone(&midi_conn),
                bpm,
                loop_beats,
            );
        }
    });

    if show_gui {
        // Create the GUI app
        let app = PatternVisualizerApp::new(
            Arc::clone(&gui_patterns), 
            Arc::clone(&gui_current_beat), 
            Arc::clone(&gui_ready),
            bpm,
        );
        let options = eframe::NativeOptions::default();

        // Run the GUI
        let result = eframe::run_native(
            "Pattern Visualizer", 
            options, 
            Box::new(move |_cc| {
                Box::new(app)
            }
        ));
        println!("All done. Exiting now... {}", result.is_err());
    } else {
        gui_ready.store(true, Ordering::SeqCst);
    }

    match playback_handle.join() {
        Ok(_) => println!("Playback finished"),
        Err(e) => println!("Playback encountered an error: {:?}", e),
    }

    Ok(())
}
