use std::{sync::{atomic::{AtomicBool, Ordering}, Arc, RwLock}, time::Duration};

use eframe::egui;

use crate::model::Pattern;

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