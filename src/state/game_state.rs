#[derive(Default, Clone, Copy, Debug)]
pub enum RunState {
    #[default]
    NotRunning,
    Running(u32), // pid
}
