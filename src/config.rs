use std::{fs::File, io::BufReader};

use serde::Deserialize;

#[derive(Deserialize)]
pub struct MidiTrackConfig {
    pub midi_file: String,
    pub track_name: String,
    pub limit_beats: f32,
}

#[derive(Deserialize)]
pub struct SoundConfig {
    pub samples: String,
    pub loops: String,
}

#[derive(Deserialize)]
pub struct Config {
    pub midi_port: String,
    pub midi_track: MidiTrackConfig,
    pub sounds: SoundConfig,
}

pub fn read_config(file_path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let config: Config = serde_json::from_reader(reader)?;
    Ok(config)
}