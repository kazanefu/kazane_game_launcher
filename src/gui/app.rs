use crate::data::remote::GameListEntry;
use crate::state::AppState;
use eframe::{egui, icon_data::from_png_bytes};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::sync::Arc;

pub struct LauncherGui {
    app_state: Arc<AppState>,
    search_query: String,
    tag_query: String,
    results: Vec<GameListEntry>,
    status: String,
    #[allow(dead_code)]
    show_logs: bool,
    current_view: ViewMode,
    last_log: Option<String>,
    readme: ReadmeEachMode,
}

#[derive(Default)]
struct ReadmeEachMode {
    cache: CommonMarkCache,
    library: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ViewMode {
    Library,
    Search,
    Logs,
}

impl LauncherGui {
    pub fn new(app_state: Arc<AppState>) -> Self {
        Self {
            app_state,
            search_query: String::new(),
            tag_query: String::new(),
            results: Vec::new(),
            status: String::new(),
            show_logs: false,
            current_view: ViewMode::Library,
            last_log: None,
            readme: ReadmeEachMode::default(),
        }
    }

    fn perform_search_with_tags(&mut self, query: &str, tags: Option<&[&str]>) {
        match self.app_state.launcher_api.search_games(query, tags) {
            Ok(v) => {
                self.results = v;
                self.status = format!("{} results", self.results.len());
                self.app_state.append_log(
                    "INFO",
                    &format!(
                        "search '{}' tags={:?} -> {} results",
                        query,
                        tags,
                        self.results.len()
                    ),
                );
            }
            Err(e) => {
                // log error in detail and show friendly status
                let msg = format!("search error for '{}': {}", query, e);
                self.app_state.append_log("ERROR", &msg);
                self.status = "Search failed (see runtime.log)".to_string();
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
            let parts_vec: Vec<String> = self
                .tag_query
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();
            let parts_ref: Vec<&str> = parts_vec.iter().map(|s| s.as_str()).collect();
            self.perform_search_with_tags(&q, Some(parts_ref.as_slice()));
        }
    }

    fn render_navigation(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                let views = [
                    ("Library", ViewMode::Library),
                    ("Search", ViewMode::Search),
                    ("Logs", ViewMode::Logs),
                ];

                for (label, mode) in views {
                    let is_active = self.current_view == mode;
                    let mut btn = egui::Button::new(
                        egui::RichText::new(label)
                            .strong()
                            .color(if is_active { egui::Color32::WHITE } else { ui.visuals().widgets.inactive.fg_stroke.color }),
                    );

                    if is_active {
                        btn = btn.fill(egui::Color32::from_rgb(0, 120, 215));
                    }

                    if ui.add(btn).clicked() {
                        self.current_view = mode;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.status.is_empty() {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(&self.status)
                                .small()
                                .italics()
                                .color(ui.visuals().weak_text_color()),
                        );
                    }
                });
            });
            ui.add_space(5.0);
        });
    }

    fn render_library_view(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.heading(egui::RichText::new("Library").strong().size(24.0));
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                let local = crate::data::local::LocalGameData::load(
                    &self.app_state.launcher_api.game_data_path,
                )
                .unwrap_or_default();

                if local.installed.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("No games installed yet.").italics());
                        if ui.button("Go to Search").clicked() {
                            self.current_view = ViewMode::Search;
                        }
                    });
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for ig in &local.installed {
                        egui::Frame::group(ui.style())
                            .fill(ui.visuals().extreme_bg_color)
                            .corner_radius(8.0)
                            .inner_margin(12.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(egui::RichText::new(&ig.name).strong().size(18.0));
                                        ui.label(
                                            egui::RichText::new(format!("version: {}", ig.version))
                                                .small()
                                                .color(ui.visuals().weak_text_color()),
                                        );
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("Launch").clicked() {
                                            self.app_state.launch_game_detached(
                                                ig.id.clone(),
                                                std::path::PathBuf::from(&ig.install_path),
                                                ig.exe_path.as_ref().map(std::path::PathBuf::from),
                                            );
                                        }
                                        if ui.button("Uninstall").clicked() {
                                            let id = ig.id.clone();
                                            let app2 = self.app_state.clone();
                                            self.status = "uninstalling...".to_string();
                                            std::thread::spawn(move || match app2.uninstall_game_by_id(&id) {
                                                Ok(_) => app2.append_log("INFO", &format!("uninstalled {}", id)),
                                                Err(e) => app2.append_log("ERROR", &format!("uninstall error {}: {}", id, e)),
                                            });
                                        }
                                        if ui.button("Show README").clicked() {
                                            let id = ig.id.clone();
                                            let app2 = self.app_state.clone();
                                            self.status = "fetching README...".to_string();
                                            let readme_handle = std::thread::spawn(move || {
                                                let rt = tokio::runtime::Runtime::new().expect("tokio");
                                                let readme = rt.block_on(app2.get_readme_by_local_id(&id));
                                                if let Err(e) = &readme {
                                                    app2.append_log("ERROR", &format!("error fetching README for {}: {}", id, e));
                                                } else {
                                                    app2.append_log("INFO", &format!("fetched README for {}", id));
                                                }
                                                readme
                                            });
                                            let readme_result = readme_handle
                                                .join()
                                                .unwrap_or_else(|_| Ok("thread panicked".into()));
                                            self.readme.library =
                                                Some(readme_result.unwrap_or_else(|e| format!("error fetching README: {}", e)));
                                        }
                                    });
                                });
                            });
                        ui.add_space(10.0);
                    }

                    if let Some(readme) = &self.readme.library {
                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.heading("About this game");
                        ui.add_space(10.0);
                        egui::Frame::canvas(ui.style()).inner_margin(10.0).show(ui, |ui| {
                            CommonMarkViewer::new().show(ui, &mut self.readme.cache, readme);
                        });
                    }
                });
            });
        });
    }

    fn render_search_view(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Search:").strong());
                    let search_edit = ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Game title..."));
                    if search_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.perform_search();
                    }
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Tags:").strong());
                    let tag_edit = ui.add(egui::TextEdit::singleline(&mut self.tag_query).hint_text("action, puzzle..."));
                    if tag_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.perform_search();
                    }
                    if ui.button("Search").clicked() {
                        self.perform_search();
                    }
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.results.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label(egui::RichText::new("Try searching for something!").italics());
                        });
                    }

                    for entry in &self.results {
                        let local = crate::data::local::LocalGameData::load(
                            &self.app_state.launcher_api.game_data_path,
                        )
                        .unwrap_or_default();
                        let installed = local.find(&entry.id).cloned();

                        egui::Frame::group(ui.style())
                            .fill(ui.visuals().extreme_bg_color)
                            .corner_radius(8.0)
                            .inner_margin(12.0)
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(egui::RichText::new(&entry.name).strong().size(18.0));
                                            ui.label(
                                                egui::RichText::new(format!("id: {}", &entry.id))
                                                    .small()
                                                    .color(ui.visuals().weak_text_color()),
                                            );
                                        });

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if installed.is_none() {
                                                if ui.button("Install").clicked() {
                                                    let id = entry.id.clone();
                                                    let app2 = self.app_state.clone();
                                                    self.status = "starting install...".to_string();
                                                    std::thread::spawn(move || {
                                                        let rt = tokio::runtime::Runtime::new().expect("tokio");
                                                        let res = rt.block_on(app2.install_game_by_id(&id));
                                                        if let Err(e) = res {
                                                            app2.append_log("ERROR", &format!("install error for {}: {}", id, e));
                                                        } else {
                                                            app2.append_log("INFO", &format!("installed {}", id));
                                                        }
                                                    });
                                                }
                                            } else {
                                                if ui.button("Launch").clicked() && let Some(ig) = &installed {
                                                    self.app_state.launch_game_detached(
                                                        ig.id.clone(),
                                                        std::path::PathBuf::from(&ig.install_path),
                                                        ig.exe_path.as_ref().map(std::path::PathBuf::from),
                                                    );
                                                }
                                                if ui.button("Update").clicked() {
                                                    let id = entry.id.clone();
                                                    let app2 = self.app_state.clone();
                                                    self.status = "updating...".to_string();
                                                    std::thread::spawn(move || {
                                                        let rt = tokio::runtime::Runtime::new().expect("tokio");
                                                        let res = rt.block_on(app2.update_game_by_id(&id));
                                                        match res {
                                                            Ok(Some(_)) => app2.append_log("INFO", &format!("updated {}", id)),
                                                            Ok(None) => app2.append_log("INFO", &format!("no update for {}", id)),
                                                            Err(e) => app2.append_log("ERROR", &format!("update error {}: {}", id, e)),
                                                        }
                                                    });
                                                }
                                                if ui.button("Uninstall").clicked() {
                                                    let id = entry.id.clone();
                                                    let app2 = self.app_state.clone();
                                                    self.status = "uninstalling...".to_string();
                                                    std::thread::spawn(move || match app2.uninstall_game_by_id(&id) {
                                                        Ok(_) => app2.append_log("INFO", &format!("uninstalled {}", id)),
                                                        Err(e) => app2.append_log("ERROR", &format!("uninstall error {}: {}", id, e)),
                                                    });
                                                }
                                            }
                                            if ui.button("README").clicked() {
                                                let id = entry.id.clone();
                                                let app2 = self.app_state.clone();
                                                self.status = "fetching README...".to_string();
                                                let readme_handle = std::thread::spawn(move || {
                                                    let rt = tokio::runtime::Runtime::new().expect("tokio");
                                                    let readme = rt.block_on(app2.get_readme_by_id(&id));
                                                    if let Err(e) = &readme {
                                                        app2.append_log("ERROR", &format!("error fetching README for {}: {}", id, e));
                                                    } else {
                                                        app2.append_log("INFO", &format!("fetched README for {}", id));
                                                    }
                                                    readme
                                                });
                                                let readme_result = readme_handle
                                                    .join()
                                                    .unwrap_or_else(|_| Ok("thread panicked".into()));
                                                self.readme.search =
                                                    Some(readme_result.unwrap_or_else(|e| format!("error fetching README: {}", e)));
                                            }
                                        });
                                    });

                                    if let Some(desc) = &entry.description {
                                        ui.add_space(5.0);
                                        ui.label(desc);
                                    }
                                    if !entry.tags.is_empty() {
                                        ui.add_space(5.0);
                                        ui.horizontal(|ui| {
                                            for tag in &entry.tags {
                                                ui.add(egui::Label::new(
                                                    egui::RichText::new(tag)
                                                        .small()
                                                        .color(egui::Color32::from_rgb(100, 200, 255)),
                                                ));
                                            }
                                        });
                                    }
                                });
                            });
                        ui.add_space(10.0);
                    }

                    if let Some(readme) = &self.readme.search {
                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.heading("About this game");
                        ui.add_space(10.0);
                        egui::Frame::canvas(ui.style()).inner_margin(10.0).show(ui, |ui| {
                            CommonMarkViewer::new().show(ui, &mut self.readme.cache, readme);
                        });
                    }
                });
            });
        });
    }

    fn render_logs_view(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Logs");
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(400.0)
                .show(ui, |ui| {
                    let logs = self.app_state.get_logs();
                    for l in logs.iter().rev().take(1000) {
                        ui.code(l);
                    }
                });
        });
    }
}

impl eframe::App for LauncherGui {
    // Use the newer ui method to avoid deprecated update warnings
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // apply theme each frame to ensure eframe doesn't override it
        let ctx = ui.ctx();
        if self.app_state.settings.theme.to_lowercase() == "dark" {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }
        // update status from latest log so UI reflects completion promptly
        let logs = self.app_state.get_logs();
        if let Some(latest) = logs.last()
            && self.last_log.as_deref() != Some(latest.as_str())
        {
            // show concise last log message in status
            self.status = latest.clone();
            self.last_log = Some(latest.clone());
        }

        self.render_navigation(ui);

        match self.current_view {
            ViewMode::Library => self.render_library_view(ui),
            ViewMode::Search => self.render_search_view(ui),
            ViewMode::Logs => self.render_logs_view(ui),
        }
    }
}

/// Start the GUI. This function is synchronous and will run the eframe event loop.
pub fn run_gui(app_state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let icon_bytes = include_bytes!("../../assets/icon/launcher_icon.png");
    let icon = from_png_bytes(icon_bytes).unwrap();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kazane Game Launcher")
            .with_icon(icon),
        ..Default::default()
    };

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
            setup_fonts(ctx);
            ctx.set_pixels_per_point(app_state_clone.settings.size);
            Ok(Box::new(LauncherGui::new(app_state_clone.clone())))
        }),
    )?;
    Ok(())
}
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "jp_font".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoSansJP-Regular.ttf"))
            .into(),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "jp_font".to_owned());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("jp_font".to_owned());

    ctx.set_fonts(fonts);
}
