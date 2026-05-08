use crate::data::local::Settings;
use crate::data::local::LocalGameData;

pub struct AppState {
    pub settings: Settings,
    pub local: LocalGameData,
}

impl AppState {
    pub fn new(settings: Settings, local: LocalGameData) -> Self {
        Self { settings, local }
    }
}
