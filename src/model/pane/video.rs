pub struct State {
    pub counter: u64,
}

impl Default for State {
    fn default() -> Self {
        Self { counter: 0 }
    }
}
