use eframe::egui;
use std::sync::Arc;
use crate::state::AppState;
use crate::data::remote::GameListEntry;

pub struct LauncherGui {
    app_state: Arc<AppState>,
    search_query: String,
    tag_query: String,
    results: Vec<GameListEntry>,
    status: String,
}

impl LauncherGui {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self { app_state, search_query: String::new(), tag_query: String::new(), results: Vec::new(), status: String::new() }
    }

    fn perform_search_with_tags(&mut self, query: &str, tags: Option<&[&str]>) {
        match self.app_state.launcher_api.search_games(query, tags) {
            Ok(v) => {
                self.results = v;
                self.status = format!("{} results", self.results.len());
            }
            Err(e) => {
                self.status = format!("search error: {}", e);
                self.results.clear();
            }
        }
    }

    fn perform_search(&mut self) {
        let q = self.search_query.clone();
        if self.tag_query.trim().is_empty() {
            self.perform_search_with_tags(&q, None);
        } else {
            // build tags vector and call immediately so references live
            let parts_vec: Vec<String> = self.tag_query.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect();
            let parts_ref: Vec<&str> = parts_vec.iter().map(|s| s.as_str()).collect();
            self.perform_search_with_tags(&q, Some(parts_ref.as_slice()));
        }
    }
}

impl eframe::App for LauncherGui {
    // Use the newer ui method to avoid deprecated update warnings
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // use the provided Ui as parent for show_inside

        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                let search_edit = ui.text_edit_singleline(&mut self.search_query);
                if search_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.perform_search();
                }
                ui.label("Tags (comma):");
                let tag_edit = ui.text_edit_singleline(&mut self.tag_query);
                if tag_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.perform_search();
                }
                if ui.button("Search").clicked() {
                    self.perform_search();
                }
                ui.label(&self.status);
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Games");
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for entry in &self.results {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(&entry.name).strong());
                            ui.label(format!("id: {}", &entry.id));
                            if let Some(desc) = &entry.description {
                                ui.label(desc);
                            }
                            if !entry.tags.is_empty() {
                                ui.label(format!("tags: {}", entry.tags.join(", ")));
                            }
                        });
                        ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                            if ui.button("Install").clicked() {
                                let id = entry.id.clone();
                                let app = self.app_state.clone();
                                // spawn async install in background
                                std::thread::spawn(move || {
                                    let rt = tokio::runtime::Runtime::new().expect("tokio");
                                    let _ = rt.block_on(async move {
                                        let _ = app.install_game_by_id(&id).await;
                                    });
                                });
                            }
                            if ui.button("Uninstall").clicked() {
                                if let Err(e) = self.app_state.uninstall_game_by_id(&entry.id) {
                                    self.status = format!("uninstall error: {}", e);
                                } else {
                                    self.status = "uninstalled".to_string();
                                }
                            }
                            if ui.button("Update").clicked() {
                                let id = entry.id.clone();
                                let app = self.app_state.clone();
                                std::thread::spawn(move || {
                                    let rt = tokio::runtime::Runtime::new().expect("tokio");
                                    let _ = rt.block_on(async move {
                                        let _ = app.update_game_by_id(&id).await;
                                    });
                                });
                            }
                        });
                    });
                    ui.separator();
                }
            });
        });
    }
}

/// Start the GUI. This function is synchronous and will run the eframe event loop.
pub fn run_gui(app_state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Kazane Game Launcher",
        native_options,
        Box::new(|_cc| Ok(Box::new(LauncherGui::new(app_state)))),
    )?;
    Ok(())
}
