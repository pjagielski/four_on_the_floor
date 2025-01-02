use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Pattern {
    pub sound: Option<String>,
    pub loop_name: Option<String>,
    pub midi_note: Option<u8>,
    pub beats: Vec<f32>,
    pub velocity: f32,
    pub duration: f32,
}

pub struct PatternBuilder {
    sound: Option<String>,
    loop_name: Option<String>,
    beats: Vec<f32>,
    midi_note: Option<u8>,
    velocity: f32,
    duration: f32,
}

impl PatternBuilder {
    pub fn new() -> Self {
        Self {
            sound: None,
            loop_name: None,
            beats: vec![],
            midi_note: None,
            velocity: 100.0,
            duration: 0.25,
        }
    }

    pub fn sound(mut self, sound: &str) -> Self {
        self.sound = Some(sound.to_string());
        self
    }

    pub fn loop_name(mut self, loop_name: &str) -> Self {
        self.loop_name = Some(loop_name.to_string());
        self
    }

    pub fn beats(mut self, beats: Vec<f32>) -> Self {
        self.beats = beats;
        self
    }

    pub fn midi_note(mut self, note: u8) -> Self {
        self.midi_note = Some(note);
        self
    }

    pub fn velocity(mut self, velocity: f32) -> Self {
        self.velocity = velocity;
        self
    }

    pub fn duration(mut self, duration: f32) -> Self {
        self.duration = duration;
        self
    }

    pub fn build(self) -> Pattern {
        Pattern {
            sound: self.sound,
            loop_name: self.loop_name,
            beats: self.beats,
            midi_note: self.midi_note,
            velocity: self.velocity,
            duration: self.duration,
        }
    }
}
