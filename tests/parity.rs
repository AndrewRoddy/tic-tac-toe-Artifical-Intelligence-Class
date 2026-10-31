use std::collections::HashSet;
use std::error::Error;

use c4::board::{Board, Geometry, Grid};
use c4::outcome::Outcome;
use c4::search::Features;
use c4::solver::Solver;

const TABLE_BITS: u32 = 16;
const WIN: usize = 0;
const DRAW: usize = 1;
const LOSS: usize = 2;

fn lone_column<G: Geometry>(board: Board<G>) -> bool {
    board.playable().all(|col| col == G::COLUMNS - 1)
}

fn quiet<G: Geometry>(board: Board<G>) -> bool {
    !board.can_win() && board.playable().all(|col| !board.opponent_wins(col))
}

fn check<G: Geometry>(
    board: Board<G>,
    solver: &mut Solver<G>,
    counts: &mut [usize; 3],
) -> Result<(), Box<dyn Error>> {
    if !quiet(board) || !lone_column(board) {
        return Ok(());
    }
    let Features([mover_good, mover_other, opp_good, opp_other]) = Features::of(board);
    if mover_good + mover_other + opp_good + opp_other != 1 {
        return Ok(());
    }
    let (expected, class) = if mover_good == 1 {
        (Outcome::Win, WIN)
    } else if opp_good == 1 {
        (Outcome::Loss, LOSS)
    } else {
        (Outcome::Draw, DRAW)
    };
    let outcome = solver.solve(board, None).ok_or("solver stopped early")?;
    assert_eq!(
        outcome,
        expected,
        "features {:?} disagree with the solver",
        [mover_good, mover_other, opp_good, opp_other]
    );
    counts[class] += 1;
    Ok(())
}

fn extend<G: Geometry>(
    board: Board<G>,
    solver: &mut Solver<G>,
    counts: &mut [usize; 3],
) -> Result<(), Box<dyn Error>> {
    let last = G::COLUMNS - 1;
    let mut line = board;
    loop {
        check(line, solver, counts)?;
        if !line.can_play(last) || line.wins(last) {
            return Ok(());
        }
        line.play(last);
    }
}

fn explore<G: Geometry>(
    board: Board<G>,
    seen: &mut HashSet<G::Bits>,
    solver: &mut Solver<G>,
    counts: &mut [usize; 3],
) -> Result<(), Box<dyn Error>> {
    if !seen.insert(board.key()) {
        return Ok(());
    }
    extend(board, solver, counts)?;
    for col in 0..G::COLUMNS - 1 {
        if board.can_play(col) && !board.wins(col) {
            let mut next = board;
            next.play(col);
            explore(next, seen, solver, counts)?;
        }
    }
    Ok(())
}

fn lone_column_rule<G: Geometry>() -> Result<(), Box<dyn Error>> {
    let mut solver = Solver::<G>::new(TABLE_BITS);
    let mut seen = HashSet::new();
    let mut counts = [0_usize; 3];
    explore(Board::<G>::default(), &mut seen, &mut solver, &mut counts)?;
    assert!(
        counts.iter().all(|&count| count > 0),
        "a result class is never reached: win, draw, loss = {counts:?}"
    );
    Ok(())
}

#[test]
fn lone_column_rule_4x4() -> Result<(), Box<dyn Error>> {
    lone_column_rule::<Grid<4, 4, u64>>()
}

#[test]
fn lone_column_rule_4x4_u128() -> Result<(), Box<dyn Error>> {
    lone_column_rule::<Grid<4, 4, u128>>()
}

#[test]
fn lone_column_rule_4x5() -> Result<(), Box<dyn Error>> {
    lone_column_rule::<Grid<4, 5, u64>>()
}

#[test]
fn lone_column_rule_5x4() -> Result<(), Box<dyn Error>> {
    lone_column_rule::<Grid<5, 4, u64>>()
}

#[test]
fn lone_column_rule_5x5() -> Result<(), Box<dyn Error>> {
    lone_column_rule::<Grid<5, 5, u64>>()
}
