use crate::data::local::LocalGameData;
use crate::data::local::Settings;

pub struct AppState {
    pub settings: Settings,
    pub local: LocalGameData,
}

impl AppState {
    pub fn new(settings: Settings, local: LocalGameData) -> Self {
        Self { settings, local }
    }
}
