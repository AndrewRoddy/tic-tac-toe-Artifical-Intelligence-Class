//! Match state and flow: settings, actions from buttons and keys, and the engine loop.
//!
//! The arena can show several boards at once ("tables"). Each table is an entity with
//! a `Game` component that owns its own engine thread, so the same two agents play
//! several matches side by side. Play, pause and step apply to every table, and every
//! result goes into one shared score.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bevy::prelude::*;

use crate::effects::{WinPop, Won};
use crate::engine::{Config, Engine, Reply, Think};
use crate::present::Mode;
use crate::roster::{DEFAULT_RED, DEFAULT_YELLOW, ROSTER};
use crate::scene::{Disc, Falling, Morphing, SceneAssets, Spot, dress_disc, halo, new_disc_look, place_disc, table_offset};
use crate::sizes::{BoardSize, DEFAULT_SIZE, LADDER};

/// Time per move: the agent's thinking budget and the minimum time a move takes.
pub const TIMES: [f32; 9] = [0.001, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0];
pub const DEFAULT_TIME: usize = 6;
/// Below this time per move discs appear in place instead of dropping.
const ANIMATE_FROM: f32 = 0.1;
/// At very short move times the engine plays about a frame's worth of moves per request.
const FRAME_SECONDS: f32 = 1.0 / 60.0;
const MAX_BATCH: u16 = 16;
pub const OPENINGS: [usize; 3] = [0, 4, 8];
const OPENING_STAGGER: f32 = 0.12;
/// Where unused pool discs wait: far below the table, out of every camera's view.
const PARKED: Vec3 = Vec3::new(0.0, -200.0, 0.0);

/// The board counts `,` and `.` step through. Past nine the boards stack into a wall.
pub const BOARD_COUNTS: [usize; 24] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 16, 20, 25, 30, 36, 42, 49, 64, 81, 100, 121, 144, 196, 256];

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
    MoreBoards,
    FewerBoards,
    PlayPause,
    Step,
    Reset,
    /// Opens the presentation (see `present.rs`).
    Present,
}

#[derive(Resource, Default)]
pub struct Pending(pub Vec<Action>);

#[derive(Resource)]
pub struct Settings {
    pub agent: [usize; 2],
    pub size: usize,
    pub time: usize,
    pub opening: usize,
    /// How many boards play at once.
    pub boards: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            agent: [DEFAULT_RED, DEFAULT_YELLOW],
            size: DEFAULT_SIZE,
            time: DEFAULT_TIME,
            opening: 0,
            boards: 1,
        }
    }
}

impl Settings {
    pub fn per_move(&self) -> f32 {
        TIMES[self.time]
    }

    pub fn board_size(&self) -> BoardSize {
        LADDER[self.size]
    }

    /// Whether discs drop in, or just appear (very fast games).
    pub fn animated(&self) -> bool {
        self.per_move() >= ANIMATE_FROM
    }

    /// Full-detail pieces only when there are few of them on screen.
    pub fn detailed(&self) -> bool {
        self.boards <= 2 && self.animated()
    }

    /// Drop animation speed: normal for slow games, up to 4x faster near `ANIMATE_FROM`.
    fn drop_speed(&self) -> Option<f32> {
        self.animated().then(|| (0.6 / self.per_move()).clamp(1.0, 4.0))
    }

    /// Moves to ask the engine for at once.
    fn batch(&self) -> u32 {
        let per_move = self.per_move();
        u32::from((1..MAX_BATCH).find(|&n| f32::from(n) * per_move >= FRAME_SECONDS).unwrap_or(MAX_BATCH))
    }

    /// How long a finished game stays on screen before the next one starts.
    pub fn hold(&self) -> f32 {
        (self.per_move() * 6.0).clamp(0.1, 1.5)
    }
}

/// The running score between the two chosen agents, and whether matches keep
/// coming (Play) or the arena is paused.
#[derive(Resource, Default)]
pub struct Series {
    pub playing: bool,
    pub wins: [u32; 2],
    pub draws: u32,
}

impl Series {
    fn clear_score(&mut self) {
        self.wins = [0, 0];
        self.draws = 0;
    }

    pub const fn games(&self) -> u32 {
        self.wins[0] + self.wins[1] + self.draws
    }
}

/// One board's match. Lives on a table entity, which also places the board in the scene.
#[derive(Component)]
pub struct Game {
    /// Which table this is: 0 is the main one the status line describes.
    pub table: usize,
    pub phase: Phase,
    pub size: BoardSize,
    grid: Vec<Option<Player>>,
    pub moves: Vec<u32>,
    pub opening_len: usize,
    pub to_move: Player,
    pub waiting: bool,
    owed: u32,
    pub think_started: f32,
    pub finished_at: f32,
    step: bool,
    start_paused: bool,
    pub outcome: Option<Outcome>,
    pub last: Option<(Player, u32, u128)>,
    pub error: Option<String>,
    engine: Option<Engine>,
    /// Disc entities this table owns (children of the table), and how many are in use.
    pool: Vec<Entity>,
    used: usize,
    /// Which pool disc sits in each cell (for the win glow), and the glow halos shown.
    cells: Vec<Option<Entity>>,
    halos: Vec<Entity>,
}

impl Game {
    fn new(table: usize, size: BoardSize) -> Self {
        Self {
            table,
            phase: Phase::Idle,
            size,
            grid: vec![None; cell_count(size)],
            moves: Vec::new(),
            opening_len: 0,
            to_move: Player::Red,
            waiting: false,
            owed: 0,
            think_started: 0.0,
            finished_at: 0.0,
            step: false,
            start_paused: false,
            outcome: None,
            last: None,
            error: None,
            engine: None,
            pool: Vec::new(),
            used: 0,
            cells: vec![None; cell_count(size)],
            halos: Vec::new(),
        }
    }

    /// The next free disc from this table's pool, or a new one spawned already wearing
    /// its mesh and material: adding `Mesh3d` to an existing entity later leaked render
    /// memory every frame in Bevy 0.20. Returns the entity and whether it is new.
    /// Slot k always holds move k, so it is always the same player and never needs a
    /// new material (changing materials on live entities leaked too).
    fn take_disc(&mut self, commands: &mut Commands, table: Entity, assets: &SceneAssets, player: Player, detailed: bool) -> (Entity, bool) {
        let taken = match self.pool.get(self.used) {
            Some(&entity) => (entity, false),
            None => {
                let entity = commands
                    .spawn((
                        Disc::spare(self.table),
                        new_disc_look(assets, player, detailed),
                        Transform::from_translation(PARKED),
                        Visibility::Inherited,
                        ChildOf(table),
                    ))
                    .id();
                self.pool.push(entity);
                (entity, true)
            }
        };
        self.used += 1;
        taken
    }

    pub const fn in_progress(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Running | Phase::Paused)
    }

    /// Running, or about to run once the agents are built.
    const fn moving(&self) -> bool {
        matches!(self.phase, Phase::Running) || (matches!(self.phase, Phase::Starting) && !self.start_paused)
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

/// The match logic; the scene and UI read the games after it each frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GameFlow;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Settings>()
            .init_resource::<Pending>()
            .init_resource::<Series>()
            .add_systems(Startup, |mut commands: Commands, settings: Res<Settings>| {
                spawn_table(&mut commands, 0, &settings);
            })
            .add_systems(
                Update,
                (keyboard.run_if(in_state(Mode::Arena)), handle_actions, sync_tables, drive_engines, next_match)
                    .chain()
                    .in_set(GameFlow),
            );
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
        (KeyCode::Period, Action::MoreBoards),
        (KeyCode::Comma, Action::FewerBoards),
        (KeyCode::KeyP, Action::Present),
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

fn spawn_table(commands: &mut Commands, table: usize, settings: &Settings) -> Entity {
    let size = settings.board_size();
    commands
        .spawn((
            Game::new(table, size),
            Spot(table_offset(table, settings.boards, size)),
            Transform::from_translation(table_offset(table, settings.boards, size)),
            Visibility::default(),
        ))
        .id()
}

/// Hides a table's discs; they go back to its pool for the next game. Discs are never
/// despawned while the table lives: at 1 ms a move, spawning thousands a second made
/// the integrated GPU's driver fall over.
fn clear(commands: &mut Commands, game: &mut Game) {
    for entity in game.halos.drain(..) {
        commands.entity(entity).despawn();
    }
    for &entity in &game.pool[..game.used] {
        // Parked out of sight rather than hidden: toggling visibility thousands of times a
        // second also leaked render memory.
        commands.entity(entity).remove::<(Falling, WinPop)>().insert((Transform::from_translation(PARKED), Disc::spare(game.table)));
    }
    game.used = 0;
}

/// A fresh game on this table, keeping its disc pool.
fn renew(commands: &mut Commands, game: &mut Game, size: BoardSize) {
    clear(commands, game);
    let pool = std::mem::take(&mut game.pool);
    *game = Game::new(game.table, size);
    game.pool = pool;
}

/// Empties a table and starts a new match on it (paused if `paused`).
fn start(commands: &mut Commands, settings: &Settings, game: &mut Game, paused: bool) {
    let size = settings.board_size();
    renew(commands, game, size);
    game.phase = Phase::Starting;
    game.start_paused = paused;
    game.engine = Some(Engine::spawn(Config {
        size,
        red: ROSTER[settings.agent[0]].entrant,
        yellow: ROSTER[settings.agent[1]].entrant,
        opening_stones: OPENINGS[settings.opening],
        seed: clock_seed() ^ (u64::try_from(game.table).unwrap_or(0) << 48),
    }));
}

/// Empties a table and leaves it idle.
fn reset(commands: &mut Commands, settings: &Settings, game: &mut Game) {
    renew(commands, game, settings.board_size());
}

#[allow(clippy::too_many_lines)]
fn handle_actions(
    mut commands: Commands,
    mut pending: ResMut<Pending>,
    mut settings: ResMut<Settings>,
    mut series: ResMut<Series>,
    mut games: Query<&mut Game>,
    mut mode: ResMut<NextState<Mode>>,
) {
    let actions: Vec<Action> = pending.0.drain(..).collect();
    // A new pairing or board size throws away the current games.
    let mut restart = false;
    for action in actions {
        match action {
            Action::Pick(player, agent) => {
                if settings.agent[player.seat()] != agent {
                    settings.agent[player.seat()] = agent;
                    series.clear_score();
                    restart = true;
                }
            }
            Action::Grow | Action::Shrink => {
                let size = if action == Action::Grow {
                    (settings.size + 1).min(LADDER.len() - 1)
                } else {
                    settings.size.saturating_sub(1)
                };
                if size != settings.size {
                    settings.size = size;
                    restart = true;
                }
            }
            Action::Slower => settings.time = (settings.time + 1).min(TIMES.len() - 1),
            Action::Faster => settings.time = settings.time.saturating_sub(1),
            Action::CycleOpening => settings.opening = (settings.opening + 1) % OPENINGS.len(),
            Action::MoreBoards => {
                settings.boards = BOARD_COUNTS.iter().copied().find(|&count| count > settings.boards).unwrap_or(settings.boards);
            }
            Action::FewerBoards => {
                settings.boards = BOARD_COUNTS.iter().rev().copied().find(|&count| count < settings.boards).unwrap_or(1);
            }
            Action::PlayPause => {
                let any_moving = games.iter().any(Game::moving);
                let any_paused = games.iter().any(|game| game.in_progress() && !game.moving());
                if any_moving {
                    // Pause everything.
                    series.playing = false;
                    for mut game in &mut games {
                        match game.phase {
                            Phase::Running => game.phase = Phase::Paused,
                            Phase::Starting => game.start_paused = true,
                            _ => {}
                        }
                    }
                } else if any_paused || !series.playing {
                    // Resume paused tables and start idle or finished ones.
                    series.playing = true;
                    for mut game in &mut games {
                        match game.phase {
                            Phase::Paused => game.phase = Phase::Running,
                            Phase::Starting => game.start_paused = false,
                            Phase::Idle | Phase::Finished => start(&mut commands, &settings, &mut game, false),
                            Phase::Running => {}
                        }
                    }
                } else {
                    // Space on the result screen stops the next matches from starting.
                    series.playing = false;
                }
            }
            Action::Step => {
                series.playing = false;
                for mut game in &mut games {
                    match game.phase {
                        Phase::Idle | Phase::Finished => {
                            start(&mut commands, &settings, &mut game, true);
                            game.step = true;
                        }
                        Phase::Running => game.phase = Phase::Paused,
                        Phase::Paused | Phase::Starting => game.step = !game.waiting,
                    }
                }
            }
            Action::Present => mode.set(Mode::Slides),
            Action::Reset => {
                for mut game in &mut games {
                    reset(&mut commands, &settings, &mut game);
                }
                series.playing = false;
                series.clear_score();
                restart = false;
            }
        }
    }
    if restart {
        for mut game in &mut games {
            if series.playing {
                start(&mut commands, &settings, &mut game, false);
            } else if game.in_progress() {
                start(&mut commands, &settings, &mut game, true);
            } else {
                reset(&mut commands, &settings, &mut game);
            }
        }
    }
}

/// Keeps one table per board in `Settings::boards`, laid out in a grid.
fn sync_tables(
    mut commands: Commands,
    settings: Res<Settings>,
    series: Res<Series>,
    mut tables: Query<(Entity, &mut Game, &mut Spot)>,
) {
    let count = tables.iter().count();
    if count == settings.boards && !settings.is_changed() {
        return;
    }
    for (entity, mut game, mut spot) in &mut tables {
        if game.table >= settings.boards {
            // Dropping the game stops its engine; its discs go with the entity.
            commands.entity(entity).despawn();
            continue;
        }
        // The scene glides each table to its new spot.
        spot.0 = table_offset(game.table, settings.boards, settings.board_size());
        if game.size != settings.board_size() && !game.in_progress() {
            reset(&mut commands, &settings, &mut game);
        }
    }
    for table in count..settings.boards {
        let entity = spawn_table(&mut commands, table, &settings);
        if series.playing {
            let mut game = Game::new(table, settings.board_size());
            start(&mut commands, &settings, &mut game, false);
            commands.entity(entity).insert(game);
        }
    }
}

/// While playing, starts each table's next match once its result has been shown.
fn next_match(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<Settings>,
    series: Res<Series>,
    mut games: Query<&mut Game>,
) {
    for mut game in &mut games {
        if game.phase == Phase::Finished && series.playing && time.elapsed_secs() >= game.finished_at + settings.hold() {
            start(&mut commands, &settings, &mut game, false);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drive_engines(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SceneAssets>,
    settings: Res<Settings>,
    mut series: ResMut<Series>,
    mut tables: Query<(Entity, &mut Game, Has<Morphing>)>,
    falling: Query<&Disc, With<Falling>>,
    mut discs: Query<&mut Mesh3d, With<Disc>>,
    mut won: MessageWriter<Won>,
) {
    for (entity, mut game, morphing) in &mut tables {
        if !game.in_progress() {
            continue;
        }
        let game = &mut *game;
        loop {
            let Some(reply) = game.engine.as_ref().and_then(Engine::poll) else {
                break;
            };
            match reply {
                Reply::Ready { opening } => {
                    let speed = settings.drop_speed();
                    for (step, &col) in opening.iter().enumerate() {
                        if let Some((player, row)) = game.drop_disc(col) {
                            let delay = f32::from(u16::try_from(step).unwrap_or(0)) * OPENING_STAGGER / speed.unwrap_or(1.0);
                            let (disc, fresh) = game.take_disc(&mut commands, entity, &assets, player, settings.detailed());
                            if !fresh && let Ok(mut mesh) = discs.get_mut(disc) {
                                dress_disc(&assets, settings.detailed(), &mut mesh);
                            }
                            place_disc(&mut commands, disc, game.table, game.size, (col, row), delay, speed);
                            if let Some(cell) = game.cells.get_mut(cell_index(game.size, col, row)) {
                                *cell = Some(disc);
                            }
                        }
                    }
                    game.opening_len = opening.len();
                    game.phase = if game.start_paused { Phase::Paused } else { Phase::Running };
                }
                Reply::Moved { col, millis, wins, full } => {
                    game.owed = game.owed.saturating_sub(1);
                    game.waiting = game.owed > 0;
                    let Some((player, row)) = game.drop_disc(col) else {
                        game.error = Some(format!("column {} is full", col + 1));
                        game.phase = Phase::Finished;
                        series.playing = false;
                        game.engine = None;
                        break;
                    };
                    let (disc, fresh) = game.take_disc(&mut commands, entity, &assets, player, settings.detailed());
                    if !fresh && let Ok(mut mesh) = discs.get_mut(disc) {
                        dress_disc(&assets, settings.detailed(), &mut mesh);
                    }
                    place_disc(&mut commands, disc, game.table, game.size, (col, row), 0.0, settings.drop_speed());
                    if let Some(cell) = game.cells.get_mut(cell_index(game.size, col, row)) {
                        *cell = Some(disc);
                    }
                    game.last = Some((player, col, millis));
                    if wins || full {
                        let outcome = if wins { Outcome::Winner(player) } else { Outcome::Draw };
                        match outcome {
                            Outcome::Winner(winner) => series.wins[winner.seat()] += 1,
                            Outcome::Draw => series.draws += 1,
                        }
                        game.outcome = Some(outcome);
                        game.phase = Phase::Finished;
                        game.waiting = false;
                        game.owed = 0;
                        game.finished_at = time.elapsed_secs();
                        game.engine = None;
                        let line = game.win_line(col, row);
                        let table = game.table;
                        // The winning discs glow: a halo spawned on each (never a material swap).
                        for &(c, r) in &line {
                            if let Some(&Some(disc)) = game.cells.get(cell_index(game.size, c, r)) {
                                let glow = halo(&mut commands, &assets, disc, player, settings.detailed());
                                game.halos.push(glow);
                            }
                        }
                        if wins {
                            won.write(Won {
                                table: entity,
                                index: table,
                                player,
                                size: game.size,
                                line,
                            });
                        }
                        break;
                    }
                }
                Reply::Failed(message) => {
                    game.error = Some(message);
                    game.phase = Phase::Finished;
                    game.waiting = false;
                    series.playing = false;
                    game.engine = None;
                    break;
                }
            }
        }
        // Wait for discs to land and for a resized board to finish changing shape.
        let landed = !morphing && !falling.iter().any(|disc| disc.table == game.table);
        let wants_move = game.phase == Phase::Running || (game.phase == Phase::Paused && game.step);
        if wants_move && !game.waiting && landed {
            let Some(engine) = game.engine.as_ref() else {
                continue;
            };
            let count = if game.phase == Phase::Paused { 1 } else { settings.batch() };
            engine.request(Think {
                per_move: Duration::from_secs_f32(settings.per_move()),
                count,
            });
            game.owed = count;
            game.waiting = true;
            game.step = false;
            game.think_started = time.elapsed_secs();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_times_batch_and_skip_the_drop() {
        let at = |time: usize| Settings { time, ..Settings::default() };
        assert_eq!(TIMES[0], 0.001);
        assert_eq!(at(0).batch(), u32::from(MAX_BATCH));
        assert_eq!(at(1).batch(), 2);
        assert_eq!(at(DEFAULT_TIME).batch(), 1);
        assert!(!at(2).animated() && at(3).animated());
        assert!(at(0).hold() >= 0.1 && at(DEFAULT_TIME).hold() <= 1.5);
    }

    #[test]
    fn finds_diagonal_and_ignores_three() {
        let mut game = Game::new(0, LADDER[3]);
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
