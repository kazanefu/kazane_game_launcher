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
    show_logs: bool,
    current_view: ViewMode,
}


enum ViewMode {
    Library,
    Search,
    Logs,
}

impl LauncherGui {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self { app_state, search_query: String::new(), tag_query: String::new(), results: Vec::new(), status: String::new(), show_logs: false, current_view: ViewMode::Library }
    }

    fn perform_search_with_tags(&mut self, query: &str, tags: Option<&[&str]>) {
        match self.app_state.launcher_api.search_games(query, tags) {
            Ok(v) => {
                self.results = v;
                self.status = format!("{} results", self.results.len());
                self.app_state.append_log("INFO", &format!("search '{}' tags={:?} -> {} results", query, tags, self.results.len()));
            }
            Err(e) => {
                // log error in detail and show friendly status
                let msg = format!("search error for '{}': {}", query, e);
                self.app_state.append_log("ERROR", &msg);
                self.status = format!("Search failed (see runtime.log)");
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
        // simple view selector
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Library").clicked() { self.current_view = ViewMode::Library; }
                if ui.button("Search").clicked() { self.current_view = ViewMode::Search; }
                if ui.button("Logs").clicked() { self.current_view = ViewMode::Logs; }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.label(&self.status);
                });
            });
        });

        match self.current_view {
            ViewMode::Library => {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.heading("Library");
                    ui.separator();
                    let local = crate::data::local::LocalGameData::load(&self.app_state.launcher_api.game_data_path).unwrap_or_default();
                    for ig in &local.installed {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&ig.name).strong());
                            ui.label(format!("version: {}", ig.version));
                            if ui.button("Launch").clicked() {
                                let id = ig.id.clone();
                                let path = std::path::PathBuf::from(&ig.install_path);
                                let app2 = self.app_state.clone();
                                self.status = "launching...".to_string();
                                std::thread::spawn(move || {
                                    let rt = tokio::runtime::Runtime::new().expect("tokio");
                                    let res = rt.block_on(app2.start_game(&id, path.clone(), &[]));
                                    match res {
                                        Ok(info) => app2.append_log("INFO", &format!("started {} pid={}", info.id, info.pid)),
                                        Err(e) => app2.append_log("ERROR", &format!("start error {}: {}", id, e)),
                                    }
                                });
                            }
                            if ui.button("Uninstall").clicked() {
                                let id = ig.id.clone();
                                let app2 = self.app_state.clone();
                                self.status = "uninstalling...".to_string();
                                std::thread::spawn(move || {
                                    match app2.uninstall_game_by_id(&id) {
                                        Ok(_) => app2.append_log("INFO", &format!("uninstalled {}", id)),
                                        Err(e) => app2.append_log("ERROR", &format!("uninstall error {}: {}", id, e)),
                                    }
                                });
                            }
                        });
                        ui.separator();
                    }
                });
            }
            ViewMode::Search => {
                egui::CentralPanel::default().show_inside(ui, |ui| {
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

                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for entry in &self.results {
                            // reload local state on each frame to reflect installs
                            let local = crate::data::local::LocalGameData::load(&self.app_state.launcher_api.game_data_path).unwrap_or_default();
                            let installed = local.find(&entry.id).cloned();

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
                                    if let Some(ig) = &installed {
                                        ui.label(format!("installed: {}", ig.version));
                                    }
                                });
                                ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                                    if installed.is_none() {
                                        if ui.button("Install").clicked() {
                                            let id = entry.id.clone();
                                            let app = self.app_state.clone();
                                            let id2 = id.clone();
                                            let app2 = app.clone();
                                            self.status = "starting install...".to_string();
                                            std::thread::spawn(move || {
                                                let rt = tokio::runtime::Runtime::new().expect("tokio");
                                                let res = rt.block_on(app2.install_game_by_id(&id2));
                                                if let Err(e) = res {
                                                    let msg = format!("install error for {}: {}", id2, e);
                                                    app2.append_log("ERROR", &msg);
                                                } else {
                                                    app2.append_log("INFO", &format!("installed {}", id2));
                                                }
                                            });
                                        }
                                    // If install completed and status still indicates installing, clear/update status
                                    if installed.is_some() && self.status.contains("install") {
                                        self.status = format!("installed {}", entry.id);
                                    }
                                    } else {
                                        if ui.button("Launch").clicked() {
                                            if let Some(ig) = &installed {
                                                let id = ig.id.clone();
                                                let path = std::path::PathBuf::from(&ig.install_path);
                                                let app = self.app_state.clone();
                                                let id2 = id.clone();
                                                let path2 = path.clone();
                                                let app2 = app.clone();
                                                self.status = "launching...".to_string();
                                                std::thread::spawn(move || {
                                                    let rt = tokio::runtime::Runtime::new().expect("tokio");
                                                    let res = rt.block_on(app2.start_game(&id2, path2.clone(), &[]));
                                                    match res {
                                                        Ok(info) => app2.append_log("INFO", &format!("started {} pid={}", info.id, info.pid)),
                                                        Err(e) => app2.append_log("ERROR", &format!("start error {}: {}", id2, e)),
                                                    }
                                                });
                                            }
                                        }
                                        if ui.button("Uninstall").clicked() {
                                            let id = entry.id.clone();
                                            let app = self.app_state.clone();
                                            let id2 = id.clone();
                                            let app2 = app.clone();
                                            self.status = "uninstalling...".to_string();
                                            std::thread::spawn(move || {
                                                match app2.uninstall_game_by_id(&id2) {
                                                    Ok(_) => app2.append_log("INFO", &format!("uninstalled {}", id2)),
                                                    Err(e) => app2.append_log("ERROR", &format!("uninstall error {}: {}", id2, e)),
                                                }
                                            });
                                        }
                                        if ui.button("Update").clicked() {
                                            let id = entry.id.clone();
                                            let app = self.app_state.clone();
                                            let id2 = id.clone();
                                            let app2 = app.clone();
                                            self.status = "updating...".to_string();
                                            std::thread::spawn(move || {
                                                let rt = tokio::runtime::Runtime::new().expect("tokio");
                                                let res = rt.block_on(app2.update_game_by_id(&id2));
                                                match res {
                                                    Ok(Some(_)) => app2.append_log("INFO", &format!("updated {}", id2)),
                                                    Ok(None) => app2.append_log("INFO", &format!("no update for {}", id2)),
                                                    Err(e) => app2.append_log("ERROR", &format!("update error {}: {}", id2, e)),
                                                }
                                            });
                                        }
                                    }
                                });
                            });
                            ui.separator();
                        }
                    });
                });
            }
            ViewMode::Logs => {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.heading("Logs");
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        let logs = self.app_state.get_logs();
                        for l in logs.iter().rev().take(1000) {
                            ui.code(l);
                        }
                    });
                });
            }
        }
    }
}

/// Start the GUI. This function is synchronous and will run the eframe event loop.
pub fn run_gui(app_state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let native_options = eframe::NativeOptions::default();
    let app_state_clone = app_state.clone();
    eframe::run_native(
        "Kazane Game Launcher",
        native_options,
        Box::new(move |cc| {
            // apply theme from settings
            let ctx = &cc.egui_ctx;
            if app_state_clone.settings.theme.to_lowercase() == "dark" {
                ctx.set_visuals(egui::Visuals::dark());
            } else {
                ctx.set_visuals(egui::Visuals::light());
            }
            Ok(Box::new(LauncherGui::new(app_state_clone.clone())))
        }),
    )?;
    Ok(())
}
