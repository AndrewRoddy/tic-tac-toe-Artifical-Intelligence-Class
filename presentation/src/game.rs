//! Match state and flow: settings, actions from buttons and keys, and the engine loop.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;

use crate::engine::{Config, Engine, Reply};
use crate::roster::{DEFAULT_RED, DEFAULT_YELLOW, ROSTER};
use crate::scene::{Disc, Falling, SceneAssets, light_up, spawn_disc};
use crate::sizes::{BoardSize, DEFAULT_SIZE, LADDER};

pub const TIMES: [f32; 6] = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0];
pub const DEFAULT_TIME: usize = 3;
pub const OPENINGS: [usize; 3] = [0, 4, 8];
const OPENING_STAGGER: f32 = 0.12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    Red,
    Yellow,
}

impl Player {
    pub const fn seat(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Yellow => 1,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Yellow => "Yellow",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Starting,
    Running,
    Paused,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Winner(Player),
    Draw,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Pick(Player, usize),
    Grow,
    Shrink,
    Faster,
    Slower,
    CycleOpening,
    PlayPause,
    Step,
    Reset,
}

impl Action {
    /// Settings that are locked while a match is in progress.
    pub const fn is_setting(self) -> bool {
        !matches!(self, Self::PlayPause | Self::Step | Self::Reset)
    }
}

#[derive(Resource, Default)]
pub struct Pending(pub Vec<Action>);

#[derive(Resource)]
pub struct Settings {
    pub agent: [usize; 2],
    pub size: usize,
    pub time: usize,
    pub opening: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            agent: [DEFAULT_RED, DEFAULT_YELLOW],
            size: DEFAULT_SIZE,
            time: DEFAULT_TIME,
            opening: 0,
        }
    }
}

impl Settings {
    pub fn per_move(&self) -> f32 {
        TIMES[self.time]
    }
}

#[derive(Resource)]
pub struct Game {
    pub phase: Phase,
    pub size: BoardSize,
    grid: Vec<Option<Player>>,
    pub moves: Vec<u32>,
    pub opening_len: usize,
    pub to_move: Player,
    pub waiting: bool,
    pub think_started: f32,
    step: bool,
    start_paused: bool,
    pub outcome: Option<Outcome>,
    pub last: Option<(Player, u32, u128)>,
    pub error: Option<String>,
}

impl Game {
    fn new(size: BoardSize) -> Self {
        Self {
            phase: Phase::Idle,
            size,
            grid: vec![None; cell_count(size)],
            moves: Vec::new(),
            opening_len: 0,
            to_move: Player::Red,
            waiting: false,
            think_started: 0.0,
            step: false,
            start_paused: false,
            outcome: None,
            last: None,
            error: None,
        }
    }

    pub const fn in_progress(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Running | Phase::Paused)
    }

    fn at(&self, col: u32, row: u32) -> Option<Player> {
        if col >= self.size.columns || row >= self.size.rows {
            return None;
        }
        self.grid[cell_index(self.size, col, row)]
    }

    fn height(&self, col: u32) -> u32 {
        (0..self.size.rows).take_while(|&row| self.at(col, row).is_some()).count().try_into().unwrap_or(0)
    }

    /// Drops a disc for the player to move. Returns its row.
    fn drop_disc(&mut self, col: u32) -> Option<(Player, u32)> {
        let row = self.height(col);
        if col >= self.size.columns || row >= self.size.rows {
            return None;
        }
        let player = self.to_move;
        self.grid[cell_index(self.size, col, row)] = Some(player);
        self.moves.push(col);
        self.to_move = match player {
            Player::Red => Player::Yellow,
            Player::Yellow => Player::Red,
        };
        Some((player, row))
    }

    /// Cells in a line of four or more through (col, row), if there is one.
    pub fn win_line(&self, col: u32, row: u32) -> Vec<(u32, u32)> {
        let Some(player) = self.at(col, row) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (dc, dr) in [(1_i64, 0_i64), (0, 1), (1, 1), (1, -1)] {
            let mut line = vec![(col, row)];
            for sign in [1_i64, -1] {
                let mut c = i64::from(col);
                let mut r = i64::from(row);
                loop {
                    c += dc * sign;
                    r += dr * sign;
                    let (Ok(cu), Ok(ru)) = (u32::try_from(c), u32::try_from(r)) else {
                        break;
                    };
                    if self.at(cu, ru) != Some(player) {
                        break;
                    }
                    line.push((cu, ru));
                }
            }
            if line.len() >= 4 {
                found.extend(line);
            }
        }
        found
    }
}

fn cell_count(size: BoardSize) -> usize {
    usize::try_from(size.columns * size.rows).unwrap_or(0)
}

fn cell_index(size: BoardSize, col: u32, row: u32) -> usize {
    usize::try_from(row * size.columns + col).unwrap_or(0)
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Settings>()
            .init_resource::<Pending>()
            .insert_resource(Game::new(LADDER[DEFAULT_SIZE]))
            .add_systems(Update, (keyboard, handle_actions, drive_engine).chain());
    }
}

fn keyboard(keys: Res<ButtonInput<KeyCode>>, mut pending: ResMut<Pending>) {
    let bindings = [
        (KeyCode::Space, Action::PlayPause),
        (KeyCode::KeyS, Action::Step),
        (KeyCode::KeyR, Action::Reset),
        (KeyCode::Equal, Action::Grow),
        (KeyCode::NumpadAdd, Action::Grow),
        (KeyCode::Minus, Action::Shrink),
        (KeyCode::NumpadSubtract, Action::Shrink),
        (KeyCode::BracketRight, Action::Slower),
        (KeyCode::BracketLeft, Action::Faster),
        (KeyCode::KeyO, Action::CycleOpening),
    ];
    for (key, action) in bindings {
        if keys.just_pressed(key) {
            pending.0.push(action);
        }
    }
}

fn clock_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x00C4_C4C4, |elapsed| u64::try_from(elapsed.as_nanos() & u128::from(u64::MAX)).unwrap_or(0))
}

fn handle_actions(
    mut commands: Commands,
    mut pending: ResMut<Pending>,
    mut settings: ResMut<Settings>,
    mut game: ResMut<Game>,
    discs: Query<Entity, With<Disc>>,
) {
    let actions: Vec<Action> = pending.0.drain(..).collect();
    for action in actions {
        if action.is_setting() && game.in_progress() {
            continue;
        }
        match action {
            Action::Pick(player, agent) => settings.agent[player.seat()] = agent,
            Action::Grow => settings.size = (settings.size + 1).min(LADDER.len() - 1),
            Action::Shrink => settings.size = settings.size.saturating_sub(1),
            Action::Slower => settings.time = (settings.time + 1).min(TIMES.len() - 1),
            Action::Faster => settings.time = settings.time.saturating_sub(1),
            Action::CycleOpening => settings.opening = (settings.opening + 1) % OPENINGS.len(),
            Action::PlayPause => match game.phase {
                Phase::Idle | Phase::Finished => start(&mut commands, &settings, &mut game, &discs, false),
                Phase::Running => game.phase = Phase::Paused,
                Phase::Paused => game.phase = Phase::Running,
                Phase::Starting => game.start_paused = !game.start_paused,
            },
            Action::Step => match game.phase {
                Phase::Idle | Phase::Finished => {
                    start(&mut commands, &settings, &mut game, &discs, true);
                    game.step = true;
                }
                Phase::Running => game.phase = Phase::Paused,
                Phase::Paused | Phase::Starting => game.step = !game.waiting,
            },
            Action::Reset => {
                commands.remove_resource::<Engine>();
                clear(&mut commands, &discs);
                *game = Game::new(LADDER[settings.size]);
            }
        }
        // Resizing between matches shows the new empty board right away.
        if !game.in_progress() && game.size != LADDER[settings.size] {
            clear(&mut commands, &discs);
            *game = Game::new(LADDER[settings.size]);
        }
    }
}

fn clear(commands: &mut Commands, discs: &Query<Entity, With<Disc>>) {
    for entity in discs {
        commands.entity(entity).despawn();
    }
}

fn start(commands: &mut Commands, settings: &Settings, game: &mut Game, discs: &Query<Entity, With<Disc>>, paused: bool) {
    clear(commands, discs);
    let size = LADDER[settings.size];
    *game = Game::new(size);
    game.phase = Phase::Starting;
    game.start_paused = paused;
    commands.insert_resource(Engine::spawn(Config {
        size,
        red: ROSTER[settings.agent[0]].entrant,
        yellow: ROSTER[settings.agent[1]].entrant,
        per_move: Duration::from_secs_f32(settings.per_move()),
        opening_stones: OPENINGS[settings.opening],
        seed: clock_seed(),
    }));
}

fn drive_engine(
    mut commands: Commands,
    time: Res<Time>,
    engine: Option<Res<Engine>>,
    assets: Res<SceneAssets>,
    mut game: ResMut<Game>,
    falling: Query<(), With<Falling>>,
    mut discs: Query<(&Disc, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let Some(engine) = engine else {
        return;
    };
    if !game.in_progress() {
        return;
    }
    while let Some(reply) = engine.poll() {
        match reply {
            Reply::Ready { opening } => {
                for (step, &col) in opening.iter().enumerate() {
                    if let Some((player, row)) = game.drop_disc(col) {
                        let delay = f32::from(u16::try_from(step).unwrap_or(0)) * OPENING_STAGGER;
                        spawn_disc(&mut commands, &assets, game.size, player, col, row, delay);
                    }
                }
                game.opening_len = opening.len();
                game.phase = if game.start_paused { Phase::Paused } else { Phase::Running };
            }
            Reply::Moved { col, millis, wins, full } => {
                game.waiting = false;
                let Some((player, row)) = game.drop_disc(col) else {
                    game.error = Some(format!("column {} is full", col + 1));
                    game.phase = Phase::Finished;
                    break;
                };
                spawn_disc(&mut commands, &assets, game.size, player, col, row, 0.0);
                game.last = Some((player, col, millis));
                if wins || full {
                    game.outcome = Some(if wins { Outcome::Winner(player) } else { Outcome::Draw });
                    game.phase = Phase::Finished;
                    commands.remove_resource::<Engine>();
                    let line = game.win_line(col, row);
                    for (disc, mut material) in &mut discs {
                        if line.contains(&(disc.col, disc.row)) {
                            light_up(&assets, disc, &mut material);
                        }
                    }
                    // The last disc was spawned this frame; give it the glow too.
                    if line.contains(&(col, row)) {
                        commands.queue(move |world: &mut World| glow_new_disc(world, col, row));
                    }
                    break;
                }
            }
            Reply::Failed(message) => {
                game.error = Some(message);
                game.phase = Phase::Finished;
                game.waiting = false;
                commands.remove_resource::<Engine>();
                break;
            }
        }
    }
    let wants_move = game.phase == Phase::Running || (game.phase == Phase::Paused && game.step);
    if wants_move && !game.waiting && falling.is_empty() {
        engine.request_move();
        game.waiting = true;
        game.step = false;
        game.think_started = time.elapsed_secs();
    }
}

fn glow_new_disc(world: &mut World, col: u32, row: u32) {
    let Some(assets) = world.get_resource::<SceneAssets>().map(SceneAssets::glow_handles) else {
        return;
    };
    let mut query = world.query::<(&Disc, &mut MeshMaterial3d<StandardMaterial>)>();
    for (disc, mut material) in query.iter_mut(world) {
        if disc.col == col && disc.row == row {
            material.0 = assets[disc.player.seat()].clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_diagonal_and_ignores_three() {
        let mut game = Game::new(LADDER[3]);
        // Red builds a rising diagonal from column 0; Yellow fills under it.
        for col in [0, 1, 1, 2, 2, 3, 2, 3, 3, 6] {
            game.drop_disc(col);
        }
        assert!(game.win_line(2, 2).is_empty(), "only three so far");
        game.drop_disc(3);
        let line = game.win_line(3, 3);
        assert_eq!(line.len(), 4, "{line:?}");
        for cell in [(0, 0), (1, 1), (2, 2), (3, 3)] {
            assert!(line.contains(&cell), "{cell:?} missing from {line:?}");
        }
    }
}
