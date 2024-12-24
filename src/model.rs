#[derive(Debug)]
pub struct Pattern {
    pub sound: Option<String>,
    pub midi_note: Option<u8>,
    pub beats: Vec<f32>,
    pub velocity: f32,
    pub duration: f32,
}