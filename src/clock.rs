use std::time::Instant;

const CLOCK_INTERVAL: u64 = 1024;

pub struct Clock {
    deadline: Option<Instant>,
    nodes: u64,
}

impl Clock {
    #[must_use]
    pub const fn new(deadline: Option<Instant>) -> Self {
        Self { deadline, nodes: 0 }
    }

    pub fn expired(&mut self) -> bool {
        let poll = self.nodes.is_multiple_of(CLOCK_INTERVAL);
        self.nodes += 1;
        poll && self.deadline.is_some_and(|limit| Instant::now() >= limit)
    }

    #[must_use]
    pub const fn nodes(&self) -> u64 {
        self.nodes
    }
}
