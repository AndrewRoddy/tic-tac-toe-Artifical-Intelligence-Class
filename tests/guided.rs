use std::error::Error;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use c4::agent::Network;
use c4::board::{B87, B98, Board, Geometry};
use c4::dataset::{ColumnLabelled, parse_column_labelled, split};
use c4::mcts::{Budget, Uct};
use c4::outcome::Outcome;

const DATA_8X7: &str = "data/train_8x7.txt";
const DATA_9X8: &str = "data/late_9x8.txt";
const SIMULATIONS: u32 = 200;
const POSITIONS: usize = 12;
const WALK_PLIES: usize = 30;
const UNLIMITED: Duration = Duration::from_secs(3600);

fn budget() -> Budget {
    Budget {
        deadline: Instant::now() + UNLIMITED,
        simulations: Some(SIMULATIONS),
    }
}

fn guided<G: Geometry>() -> Result<Uct<G, Network>, Box<dyn Error>> {
    Ok(Uct::with_guide(
        Network::shipped().ok_or("shipped network")?,
    ))
}

fn held_out<G: Geometry>(data: &str) -> Result<Vec<ColumnLabelled<G>>, Box<dyn Error>> {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(data))?;
    let mut labelled = Vec::new();
    for line in split(text.lines().collect()).held {
        labelled.push(parse_column_labelled::<G>(line).ok_or("malformed labelled position")?);
    }
    Ok(labelled)
}

fn walk_to_threat<G: Geometry>(mut board: Board<G>) -> Option<Board<G>> {
    for _ in 0..WALK_PLIES {
        if board.can_win() {
            return Some(board);
        }
        let col = board.centre_order().find(|&col| !board.wins(col))?;
        board.play(col);
    }
    None
}

fn threat_positions<G: Geometry>(data: &str) -> Result<Vec<Board<G>>, Box<dyn Error>> {
    Ok(held_out::<G>(data)?
        .into_iter()
        .filter_map(|labelled| walk_to_threat(labelled.position.board))
        .take(POSITIONS)
        .collect())
}

fn immediate_wins<G: Geometry>(board: Board<G>) -> Vec<u32> {
    board.playable().filter(|&col| board.wins(col)).collect()
}

fn safe_columns<G: Geometry>(board: Board<G>) -> Vec<u32> {
    board
        .playable()
        .filter(|&col| {
            let mut next = board;
            next.play(col);
            !next.playable().any(|reply| next.wins(reply))
        })
        .collect()
}

fn label(columns: &[Option<Outcome>], column: u32) -> Option<Outcome> {
    usize::try_from(column)
        .ok()
        .and_then(|index| columns.get(index).copied().flatten())
}

fn random_safe_expectation(
    columns: &[Option<Outcome>],
    safe: &[u32],
    best: Outcome,
) -> Result<f64, Box<dyn Error>> {
    let right = safe
        .iter()
        .filter(|&&col| label(columns, col) == Some(best))
        .count();
    Ok(f64::from(u32::try_from(right)?) / f64::from(u32::try_from(safe.len())?))
}

fn finds_immediate_win<G: Geometry>(data: &str) -> Result<(), Box<dyn Error>> {
    let mut uct = guided::<G>()?;
    let boards = threat_positions::<G>(data)?;
    assert_eq!(boards.len(), POSITIONS);
    for board in boards {
        let wins = immediate_wins(board);
        assert!(!wins.is_empty());
        assert!(
            uct.search(board, budget())
                .is_some_and(|col| wins.contains(&col))
        );
    }
    Ok(())
}

fn blocks_only_safe_column<G: Geometry>(data: &str) -> Result<(), Box<dyn Error>> {
    let mut uct = guided::<G>()?;
    let mut checked = 0_usize;
    for labelled in held_out::<G>(data)? {
        let board = labelled.position.board;
        if board.playable().count() < 2 || !immediate_wins(board).is_empty() {
            continue;
        }
        let safe = safe_columns(board);
        let &[only] = safe.as_slice() else {
            continue;
        };
        checked += 1;
        assert_eq!(uct.search(board, budget()), Some(only));
        if checked == POSITIONS {
            break;
        }
    }
    assert_eq!(checked, POSITIONS);
    Ok(())
}

fn beats_random_safe_pick<G: Geometry>(data: &str) -> Result<(), Box<dyn Error>> {
    let mut uct = guided::<G>()?;
    let mut positions = 0_usize;
    let mut guided_right = 0_u32;
    let mut random_right = 0.0_f64;
    for labelled in held_out::<G>(data)? {
        let board = labelled.position.board;
        let safe = safe_columns(board);
        let Some(best) = labelled.columns.iter().flatten().max().copied() else {
            continue;
        };
        if !immediate_wins(board).is_empty() || safe.len() < 2 || best == Outcome::Loss {
            continue;
        }
        let column = uct
            .search(board, budget())
            .ok_or("search returned no column")?;
        positions += 1;
        guided_right += u32::from(label(&labelled.columns, column) == Some(best));
        random_right += random_safe_expectation(&labelled.columns, &safe, best)?;
        if positions == POSITIONS {
            break;
        }
    }
    assert_eq!(positions, POSITIONS);
    assert!(
        f64::from(guided_right) > random_right,
        "guided {guided_right} of {POSITIONS}, random expectation {random_right:.2}"
    );
    Ok(())
}

#[test]
fn guided_finds_win_8x7() -> Result<(), Box<dyn Error>> {
    finds_immediate_win::<B87>(DATA_8X7)
}

#[test]
fn guided_blocks_safe_8x7() -> Result<(), Box<dyn Error>> {
    blocks_only_safe_column::<B87>(DATA_8X7)
}

#[test]
fn guided_beats_random_8x7() -> Result<(), Box<dyn Error>> {
    beats_random_safe_pick::<B87>(DATA_8X7)
}

#[test]
fn guided_beats_random_9x8() -> Result<(), Box<dyn Error>> {
    beats_random_safe_pick::<B98>(DATA_9X8)
}
