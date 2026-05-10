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

        // use the provided Ui as parent for show_inside
        // simple view selector
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Library").clicked() {
                    self.current_view = ViewMode::Library;
                }
                if ui.button("Search").clicked() {
                    self.current_view = ViewMode::Search;
                }
                if ui.button("Logs").clicked() {
                    self.current_view = ViewMode::Logs;
                }
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
                    let local = crate::data::local::LocalGameData::load(
                        &self.app_state.launcher_api.game_data_path,
                    )
                    .unwrap_or_default();
                    for ig in &local.installed {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&ig.name).strong());
                            ui.label(format!("version: {}", ig.version));
                                if ui.button("Launch").clicked() {
                                    let id = ig.id.clone();
                                    let install_path = std::path::PathBuf::from(&ig.install_path);
                                    let exe_path = ig.exe_path.as_ref().map(std::path::PathBuf::from);
                                    let app2 = self.app_state.clone();
                                    self.status = "launching...".to_string();
                                    std::thread::spawn(move || {
                                        let rt = tokio::runtime::Runtime::new().expect("tokio");
                                        let res = rt.block_on(app2.start_game(&id, install_path, exe_path, &[]));
                                    match res {
                                        Ok(info) => app2.append_log(
                                            "INFO",
                                            &format!("started {} pid={}", info.id, info.pid),
                                        ),
                                        Err(e) => app2.append_log(
                                            "ERROR",
                                            &format!("start error {}: {}", id, e),
                                        ),
                                    }
                                });
                            }
                            if ui.button("Uninstall").clicked() {
                                let id = ig.id.clone();
                                let app2 = self.app_state.clone();
                                self.status = "uninstalling...".to_string();
                                std::thread::spawn(move || match app2.uninstall_game_by_id(&id) {
                                    Ok(_) => {
                                        app2.append_log("INFO", &format!("uninstalled {}", id))
                                    }
                                    Err(e) => app2.append_log(
                                        "ERROR",
                                        &format!("uninstall error {}: {}", id, e),
                                    ),
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
                                        app2.append_log(
                                            "ERROR",
                                            &format!("error fetching README for {}: {}", id, e),
                                        );
                                    } else {
                                        app2.append_log(
                                            "INFO",
                                            &format!("fetched README for {}", id),
                                        );
                                    }
                                    readme
                                });
                                let readme_result = readme_handle
                                    .join()
                                    .unwrap_or_else(|_| Ok("thread panicked".into()));
                                self.readme.library =
                                    Some(readme_result.unwrap_or_else(|e| {
                                        format!("error fetching README: {}", e)
                                    }));
                            }
                        });
                        ui.separator();
                    }
                    ui.separator();
                    if let Some(readme) = &self.readme.library {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            CommonMarkViewer::new().show(ui, &mut self.readme.cache, readme);
                        });
                    }
                });
            }
            ViewMode::Search => {
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Search:");
                        let search_edit = ui.text_edit_singleline(&mut self.search_query);
                        if search_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
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
                            let local = crate::data::local::LocalGameData::load(
                                &self.app_state.launcher_api.game_data_path,
                            )
                            .unwrap_or_default();
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
                                    if ui.button("Show README").clicked() {
                                        let id = entry.id.clone();
                                        let app2 = self.app_state.clone();
                                        self.status = "fetching README...".to_string();
                                        let readme_handle = std::thread::spawn(move || {
                                            let rt = tokio::runtime::Runtime::new().expect("tokio");
                                            let readme = rt.block_on(app2.get_readme_by_id(&id));
                                            if let Err(e) = &readme {
                                                app2.append_log(
                                                    "ERROR",
                                                    &format!(
                                                        "error fetching README for {}: {}",
                                                        id, e
                                                    ),
                                                );
                                            } else {
                                                app2.append_log(
                                                    "INFO",
                                                    &format!("fetched README for {}", id),
                                                );
                                            }
                                            readme
                                        });
                                        let readme_result = readme_handle
                                            .join()
                                            .unwrap_or_else(|_| Ok("thread panicked".into()));
                                        self.readme.search =
                                            Some(readme_result.unwrap_or_else(|e| {
                                                format!("error fetching README: {}", e)
                                            }));
                                    }
                                    if installed.is_none() {
                                        if ui.button("Install").clicked() {
                                            let id = entry.id.clone();
                                            let app = self.app_state.clone();
                                            let id2 = id.clone();
                                            let app2 = app.clone();
                                            self.status = "starting install...".to_string();
                                            std::thread::spawn(move || {
                                                let rt =
                                                    tokio::runtime::Runtime::new().expect("tokio");
                                                let res =
                                                    rt.block_on(app2.install_game_by_id(&id2));
                                                if let Err(e) = res {
                                                    let msg =
                                                        format!("install error for {}: {}", id2, e);
                                                    app2.append_log("ERROR", &msg);
                                                } else {
                                                    app2.append_log(
                                                        "INFO",
                                                        &format!("installed {}", id2),
                                                    );
                                                }
                                            });
                                        }
                                        // If install completed and status still indicates installing, clear/update status
                                        if installed.is_some() && self.status.contains("install") {
                                            self.status = format!("installed {}", entry.id);
                                        }
                                    } else {
                                        if ui.button("Launch").clicked()
                                            && let Some(ig) = &installed
                                        {
                                            let id = ig.id.clone();
                                            let install_path = std::path::PathBuf::from(&ig.install_path);
                                            let exe_path = ig.exe_path.as_ref().map(std::path::PathBuf::from);
                                            let app2 = self.app_state.clone();
                                            self.status = "launching...".to_string();
                                            std::thread::spawn(move || {
                                                let rt =
                                                    tokio::runtime::Runtime::new().expect("tokio");
                                                let res = rt.block_on(app2.start_game(
                                                    &id,
                                                    install_path,
                                                    exe_path,
                                                    &[],
                                                ));
                                                match res {
                                                    Ok(info) => app2.append_log(
                                                        "INFO",
                                                        &format!(
                                                            "started {} pid={}",
                                                            info.id, info.pid
                                                        ),
                                                    ),
                                                    Err(e) => app2.append_log(
                                                        "ERROR",
                                                        &format!("start error {}: {}", id, e),
                                                    ),
                                                }
                                            });
                                        }
                                        if ui.button("Uninstall").clicked() {
                                            let id = entry.id.clone();
                                            let app = self.app_state.clone();
                                            let id2 = id.clone();
                                            let app2 = app.clone();
                                            self.status = "uninstalling...".to_string();
                                            std::thread::spawn(move || {
                                                match app2.uninstall_game_by_id(&id2) {
                                                    Ok(_) => app2.append_log(
                                                        "INFO",
                                                        &format!("uninstalled {}", id2),
                                                    ),
                                                    Err(e) => app2.append_log(
                                                        "ERROR",
                                                        &format!("uninstall error {}: {}", id2, e),
                                                    ),
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
                                                let rt =
                                                    tokio::runtime::Runtime::new().expect("tokio");
                                                let res = rt.block_on(app2.update_game_by_id(&id2));
                                                match res {
                                                    Ok(Some(_)) => app2.append_log(
                                                        "INFO",
                                                        &format!("updated {}", id2),
                                                    ),
                                                    Ok(None) => app2.append_log(
                                                        "INFO",
                                                        &format!("no update for {}", id2),
                                                    ),
                                                    Err(e) => app2.append_log(
                                                        "ERROR",
                                                        &format!("update error {}: {}", id2, e),
                                                    ),
                                                }
                                            });
                                        }
                                    }
                                });
                            });
                            ui.separator();
                        }
                        if let Some(readme) = &self.readme.search {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                CommonMarkViewer::new().show(ui, &mut self.readme.cache, readme);
                            });
                        }
                    });
                });
            }
            ViewMode::Logs => {
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
