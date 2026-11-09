//! Background worker thread that owns the agents and the board.
//!
//! The UI sends `Think` when it wants the next move (after the previous disc has
//! landed and the game is not paused). Dropping the `Engine` stops the worker after
//! its current think.

use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use bevy::prelude::Resource;
use c4::tournament::Entrant;

use crate::sizes::{BoardSize, start_match};

pub enum Reply {
    Ready { opening: Vec<u32> },
    Moved { col: u32, millis: u128, wins: bool, full: bool },
    Failed(String),
}

#[derive(Resource)]
pub struct Engine {
    think: Sender<()>,
    replies: Mutex<Receiver<Reply>>,
}

pub struct Config {
    pub size: BoardSize,
    pub red: Entrant,
    pub yellow: Entrant,
    pub per_move: Duration,
    pub opening_stones: usize,
    pub seed: u64,
}

impl Engine {
    pub fn spawn(config: Config) -> Self {
        let (think_tx, think_rx) = mpsc::channel::<()>();
        let (reply_tx, reply_rx) = mpsc::channel::<Reply>();
        thread::Builder::new()
            .name("c4-engine".to_owned())
            .spawn(move || run(&config, &think_rx, &reply_tx))
            .ok();
        Self {
            think: think_tx,
            replies: Mutex::new(reply_rx),
        }
    }

    pub fn request_move(&self) {
        self.think.send(()).ok();
    }

    /// Next reply, if one is waiting. A dead worker reports `Failed`.
    pub fn poll(&self) -> Option<Reply> {
        let replies = self.replies.lock().ok()?;
        match replies.try_recv() {
            Ok(reply) => Some(reply),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Reply::Failed("engine stopped".to_owned())),
        }
    }
}

fn run(config: &Config, think: &Receiver<()>, reply: &Sender<Reply>) {
    let started = match start_match(
        config.size,
        config.red,
        config.yellow,
        config.seed,
        config.opening_stones,
    ) {
        Ok(started) => started,
        Err(message) => {
            reply.send(Reply::Failed(message)).ok();
            return;
        }
    };
    let mut game = started.game;
    if reply.send(Reply::Ready { opening: started.opening }).is_err() {
        return;
    }
    while think.recv().is_ok() {
        let begun = Instant::now();
        let col = game.choose(begun + config.per_move);
        let millis = begun.elapsed().as_millis();
        if !game.legal(col) {
            reply.send(Reply::Failed(format!("agent chose illegal column {}", col + 1))).ok();
            return;
        }
        let wins = game.wins(col);
        game.play(col);
        let full = game.is_full();
        if reply.send(Reply::Moved { col, millis, wins, full }).is_err() || wins || full {
            return;
        }
    }
}
