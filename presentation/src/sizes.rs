//! Board size ladder and type erasure over the compile-time `Geometry` of the `c4` crate.

use std::time::Instant;

use c4::agent::Agent;
use c4::board::{Board, Geometry, Grid};
use c4::dataset::sample;
use c4::random::SplitMix;
use c4::tournament::{Entrant, PERFECT_TABLE_BITS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardSize {
    pub columns: u32,
    pub rows: u32,
    pub label: &'static str,
}

/// The sizes the presenter can step through with the grow / shrink buttons.
pub const LADDER: [BoardSize; 6] = [
    BoardSize { columns: 4, rows: 4, label: "tiny" },
    BoardSize { columns: 5, rows: 4, label: "small" },
    BoardSize { columns: 6, rows: 5, label: "medium" },
    BoardSize { columns: 7, rows: 6, label: "standard" },
    BoardSize { columns: 8, rows: 7, label: "training board" },
    BoardSize { columns: 9, rows: 8, label: "test board" },
];

pub const DEFAULT_SIZE: usize = 5;

/// A running game with both agents, independent of the board size.
pub trait Match {
    /// The agent whose turn it is picks a column before `deadline` (it may overrun slightly).
    fn choose(&mut self, deadline: Instant) -> u32;
    fn legal(&self, col: u32) -> bool;
    /// Whether playing `col` wins for the player to move.
    fn wins(&self, col: u32) -> bool;
    fn play(&mut self, col: u32);
    fn is_full(&self) -> bool;
}

struct Session<G: Geometry> {
    board: Board<G>,
    agents: [Box<dyn Agent<G>>; 2],
}

impl<G: Geometry> Match for Session<G> {
    fn choose(&mut self, deadline: Instant) -> u32 {
        let seat = usize::from(!self.board.first_to_move());
        self.agents[seat].choose(self.board, deadline)
    }

    fn legal(&self, col: u32) -> bool {
        col < G::COLUMNS && self.board.can_play(col)
    }

    fn wins(&self, col: u32) -> bool {
        self.board.wins(col)
    }

    fn play(&mut self, col: u32) {
        self.board.play(col);
    }

    fn is_full(&self) -> bool {
        self.board.empty_cells() == 0
    }
}

pub struct Started {
    pub game: Box<dyn Match>,
    pub opening: Vec<u32>,
}

fn start<G: Geometry + 'static>(
    red: Entrant,
    yellow: Entrant,
    seed: u64,
    opening_stones: usize,
) -> Result<Started, String> {
    let build = |entrant: Entrant, salt: u64| {
        entrant
            .build::<G>(seed ^ salt, PERFECT_TABLE_BITS)
            .ok_or_else(|| format!("could not build agent {}", entrant.name()))
    };
    let agents = [build(red, 0x5EED_0001)?, build(yellow, 0x5EED_0002)?];
    let (board, opening) = if opening_stones == 0 {
        (Board::<G>::default(), Vec::new())
    } else {
        let mut rng = SplitMix::new(seed);
        let found = sample::<G>(&mut rng, opening_stones)
            .ok_or_else(|| "could not find a random opening".to_owned())?;
        (found.board, found.columns)
    };
    Ok(Started {
        game: Box::new(Session { board, agents }),
        opening,
    })
}

macro_rules! dispatch {
    ($size:expr, $red:expr, $yellow:expr, $seed:expr, $stones:expr;
     $($cols:literal x $rows:literal : $bits:ty),* $(,)?) => {
        match ($size.columns, $size.rows) {
            $(($cols, $rows) => start::<Grid<$cols, $rows, $bits>>($red, $yellow, $seed, $stones),)*
            (cols, rows) => Err(format!("board size {cols}x{rows} is not on the ladder")),
        }
    };
}

/// Builds both agents and the starting board. Call this on the worker thread:
/// the agents are not `Send`.
pub fn start_match(
    size: BoardSize,
    red: Entrant,
    yellow: Entrant,
    seed: u64,
    opening_stones: usize,
) -> Result<Started, String> {
    dispatch!(size, red, yellow, seed, opening_stones;
        4 x 4: u64,
        5 x 4: u64,
        6 x 5: u64,
        7 x 6: u64,
        8 x 7: u64,
        9 x 8: u128,
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn every_ladder_size_plays_to_the_end() {
        for size in LADDER {
            for stones in [0, 4] {
                let started = start_match(size, Entrant::SafeRandom, Entrant::SafeRandom, 7, stones)
                    .unwrap_or_else(|err| panic!("{err}"));
                assert_eq!(started.opening.len(), stones);
                let mut game = started.game;
                let cells = size.columns * size.rows;
                let mut plies = u32::try_from(stones).unwrap_or(0);
                loop {
                    let col = game.choose(Instant::now() + Duration::from_millis(5));
                    assert!(game.legal(col), "illegal column {col} on {size:?}");
                    let won = game.wins(col);
                    game.play(col);
                    plies += 1;
                    if won || game.is_full() {
                        break;
                    }
                }
                assert!(plies <= cells);
            }
        }
    }

    #[test]
    fn every_agent_builds_on_the_test_board() {
        for entrant in c4::tournament::ROSTER {
            let started = start_match(LADDER[DEFAULT_SIZE], entrant, entrant, 1, 0);
            assert!(started.is_ok(), "{}", entrant.name());
        }
    }
}
