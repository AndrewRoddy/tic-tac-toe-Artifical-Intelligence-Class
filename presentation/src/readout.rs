//! Everything the arena's UI says, computed once a frame from the game state. Both the
//! 3D HUD (`hud.rs`) and the flat backup UI (`ui.rs`) draw from this, so they always agree.

use bevy::prelude::*;

use crate::game::{Game, GameFlow, OPENINGS, Outcome, Phase, Player, Series, Settings};
use crate::roster::ROSTER;
use crate::sizes::LADDER;

/// What one agent card shows.
#[derive(Default, Clone, PartialEq)]
pub struct Card {
    pub title: String,
    pub family: String,
    pub how: String,
    pub stats: [String; 3],
    /// Elo as a share of the best agent's, for the bar.
    pub elo_share: f32,
}

#[derive(Resource, Default)]
pub struct Readout {
    pub size: String,
    pub time: String,
    pub boards: String,
    pub play: String,
    pub opening: String,
    pub status: String,
    pub moves: String,
    pub banner: String,
    /// True when the banner announces a result (shown bigger and brighter).
    pub banner_result: bool,
    pub score: [u32; 2],
    pub note: String,
    /// How much of the current think time has passed (0 when not thinking).
    pub think: f32,
    pub to_move: Player,
    pub cards: [Card; 2],
}

impl Default for Player {
    fn default() -> Self {
        Self::Red
    }
}

const TOP_ELO: f32 = 740.0;

pub struct ReadoutPlugin;

impl Plugin for ReadoutPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Readout>().add_systems(Update, compute.after(GameFlow).in_set(ReadoutSet));
    }
}

/// UIs read the readout after this set.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReadoutSet;

pub fn time_label(seconds: f32) -> String {
    if seconds <= 0.001 {
        "1 ms / move".to_owned()
    } else if seconds < 0.1 {
        format!("{:.0} ms / move", seconds * 1000.0)
    } else {
        format!("{seconds:.2} s / move")
    }
}

fn card(agent: usize) -> Card {
    let info = &ROSTER[agent];
    Card {
        title: info.title.to_owned(),
        family: format!("{}  \u{b7}  id: {}", info.family.to_lowercase(), info.entrant.name()),
        how: info.how.to_owned(),
        stats: [
            format!("9x8 tournament Elo: {}", info.elo.round()),
            format!("8x7 move accuracy: {:.1}%", info.acc_8x7 * 100.0),
            format!("9x8 late-game accuracy: {:.1}%", info.acc_9x8 * 100.0),
        ],
        elo_share: (info.elo / TOP_ELO).clamp(0.01, 1.0),
    }
}

/// Sets `slot` if the value differs; returns whether it did.
fn put(slot: &mut String, value: String) -> bool {
    let differs = *slot != value;
    if differs {
        *slot = value;
    }
    differs
}

#[allow(clippy::too_many_lines)]
fn compute(time: Res<Time>, settings: Res<Settings>, series: Res<Series>, games: Query<&Game>, mut readout: ResMut<Readout>) {
    let Some(game) = games.iter().find(|game| game.table == 0) else {
        return;
    };
    let agent = |player: Player| ROSTER[settings.agent[player.seat()]].title;
    let r = readout.bypass_change_detection();
    let size = LADDER[settings.size];
    let mut changed = false;
    changed |= put(&mut r.size, format!("{} x {}  {}", size.columns, size.rows, size.label));
    changed |= put(&mut r.time, time_label(settings.per_move()));
    changed |= put(&mut r.boards, settings.boards.to_string());
    let play = match game.phase {
        Phase::Idle => "Play",
        _ if series.playing => "Pause",
        Phase::Finished => "New match",
        Phase::Running => "Pause",
        Phase::Paused | Phase::Starting => "Resume",
    };
    changed |= put(&mut r.play, play.to_owned());
    let opening = match OPENINGS[settings.opening] {
        0 => "empty board".to_owned(),
        stones => format!("random {stones} stones"),
    };
    changed |= put(&mut r.opening, opening);

    let last = game.last.map_or_else(String::new, |(player, col, millis)| {
        format!("   (last: {} played column {} in {} ms)", player.name(), col + 1, millis)
    });
    let status = if let Some(error) = games.iter().find_map(|game| game.error.as_ref()) {
        format!("Stopped: {error}")
    } else if settings.boards > 1 {
        let count = |phase: Phase| games.iter().filter(|game| game.phase == phase).count();
        let running = count(Phase::Running) + count(Phase::Starting);
        let state = if series.playing {
            "playing"
        } else if count(Phase::Paused) > 0 {
            "paused: Space to resume, S to step"
        } else {
            "press Play (Space) to start every board"
        };
        format!("{} boards, {state}.   {running} mid-game, {} finished.", settings.boards, count(Phase::Finished))
    } else {
        match game.phase {
            Phase::Idle => "Pick two agents and a board size, then press Play (Space).".to_owned(),
            Phase::Starting => "Building agents...".to_owned(),
            Phase::Running if !settings.animated() => "Racing: one match after another as fast as they can think.".to_owned(),
            Phase::Running | Phase::Paused if game.waiting => {
                format!("{} ({}) is thinking...{last}", game.to_move.name(), agent(game.to_move))
            }
            Phase::Running => format!("{} to move{last}", game.to_move.name()),
            Phase::Paused => format!("Paused: Space to resume, S to step{last}"),
            Phase::Finished => {
                let result = match game.outcome {
                    Some(Outcome::Winner(player)) => format!("{} ({}) wins!", player.name(), agent(player)),
                    Some(Outcome::Draw) => "Draw: the board is full.".to_owned(),
                    None => String::new(),
                };
                let next = if series.playing { "   Next match coming up..." } else { "   Space for a new match." };
                format!("{result}{next}")
            }
        }
    };
    changed |= put(&mut r.status, status);

    let moves: Vec<String> = game.moves.iter().map(|col| (col + 1).to_string()).collect();
    let (opening, played) = moves.split_at(game.opening_len.min(moves.len()));
    let mut listing = if settings.boards > 1 {
        format!("Board 1 moves ({}): ", game.moves.len())
    } else {
        format!("Moves ({}): ", game.moves.len())
    };
    if !opening.is_empty() {
        listing.push_str(&format!("[opening {}]  ", opening.join(" ")));
    }
    listing.push_str(&played.join(" "));
    changed |= put(&mut r.moves, listing);

    let result_phase = if settings.boards == 1 { game.phase } else { Phase::Idle };
    let (banner, is_result) = match (result_phase, game.outcome) {
        (Phase::Finished, Some(Outcome::Winner(winner))) => {
            let loser = match winner {
                Player::Red => Player::Yellow,
                Player::Yellow => Player::Red,
            };
            (
                format!(
                    "{} WINS: {} beats {} in {} moves",
                    winner.name().to_uppercase(),
                    agent(winner),
                    agent(loser),
                    game.moves.len() - game.opening_len
                ),
                true,
            )
        }
        (Phase::Finished, Some(Outcome::Draw)) => (format!("DRAW: {} vs {}", agent(Player::Red), agent(Player::Yellow)), true),
        _ => (format!("{}  vs  {}", agent(Player::Red), agent(Player::Yellow)), false),
    };
    changed |= put(&mut r.banner, banner);
    changed |= put(&mut r.note, format!("draws {}   \u{b7}   games {}", series.draws, series.games()));
    if r.banner_result != is_result || r.score != series.wins || r.to_move != game.to_move {
        changed = true;
    }
    r.banner_result = is_result;
    r.score = series.wins;
    r.to_move = game.to_move;
    for player in [Player::Red, Player::Yellow] {
        let wanted = card(settings.agent[player.seat()]);
        if r.cards[player.seat()] != wanted {
            r.cards[player.seat()] = wanted;
            changed = true;
        }
    }
    // The think bar moves every frame, so it does not count as a change.
    r.think = if game.waiting {
        ((time.elapsed_secs() - game.think_started) / settings.per_move()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if changed {
        readout.set_changed();
    }
}
