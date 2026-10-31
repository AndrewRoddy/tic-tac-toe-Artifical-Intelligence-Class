use std::collections::HashSet;

use c4::board::{Board, Geometry, Grid};
use c4::rules;

#[path = "common/oracle.rs"]
pub mod oracle;
use oracle::{Partial, grid_wins};

#[derive(Clone)]
struct Position<G: Geometry> {
    board: Board<G>,
    grid: Partial,
}

fn start<G: Geometry>() -> Position<G> {
    Position {
        board: Board::default(),
        grid: Partial::default(),
    }
}

fn mover<G: Geometry>(position: &Position<G>) -> usize {
    usize::from(!position.board.first_to_move())
}

fn advance<G: Geometry>(position: &Position<G>, col: u32) -> Position<G> {
    let mut next = position.clone();
    next.grid.place(mover(position), col);
    next.board.play(col);
    next
}

fn wins_at<G: Geometry>(position: &Position<G>, col: u32) -> bool {
    grid_wins(
        &position.grid.stones[mover(position)],
        position.grid.cell(col),
    )
}

fn opponent_wins_at<G: Geometry>(position: &Position<G>, col: u32) -> bool {
    grid_wins(
        &position.grid.stones[1 - mover(position)],
        position.grid.cell(col),
    )
}

fn allows_win<G: Geometry>(position: &Position<G>, col: u32) -> bool {
    let next = advance(position, col);
    next.board
        .playable()
        .any(|above| grid_wins(&next.grid.stones[mover(&next)], next.grid.cell(above)))
}

fn first_takes<G: Geometry>(row: i64) -> bool {
    let fill = (i64::from(G::COLUMNS) - 1) * i64::from(G::ROWS);
    (fill + row) % 2 == 0
}

fn good_row<G: Geometry>(side: usize, row: i64) -> bool {
    first_takes::<G>(row) == (side == 0)
}

fn good_threats<G: Geometry>(position: &Position<G>, side: usize) -> i64 {
    (0..G::COLUMNS)
        .flat_map(|column| {
            let height = position.grid.cell(column).1;
            (height..i64::from(G::ROWS)).map(move |row| (i64::from(column), row))
        })
        .filter(|&cell| good_row::<G>(side, cell.1) && grid_wins(&position.grid.stones[side], cell))
        .map(|_| 1_i64)
        .sum()
}

fn value<G: Geometry>(position: &Position<G>, col: u32) -> i64 {
    let next = advance(position, col);
    let side = mover(position);
    good_threats(&next, side) - good_threats(&next, 1 - side)
}

fn follow_up_applies<G: Geometry>(position: &Position<G>, col: u32) -> bool {
    position.board.can_play(col)
        && good_row::<G>(mover(position), position.grid.cell(col).1)
        && !allows_win(position, col)
}

fn audit_strategy<G: Geometry>(position: &Position<G>) {
    let choice = rules::choose(position.board, None).unwrap_or(u32::MAX);
    let playable: Vec<u32> = position.board.playable().collect();
    let wins: Vec<u32> = playable
        .iter()
        .copied()
        .filter(|&col| wins_at(position, col))
        .collect();
    if !wins.is_empty() {
        assert!(wins.contains(&choice), "must win when a win is available");
        return;
    }
    let blocks: Vec<u32> = playable
        .iter()
        .copied()
        .filter(|&col| opponent_wins_at(position, col))
        .collect();
    if !blocks.is_empty() {
        assert!(blocks.contains(&choice), "must block an opponent win");
        return;
    }
    let safe: Vec<u32> = playable
        .iter()
        .copied()
        .filter(|&col| !allows_win(position, col))
        .collect();
    if safe.is_empty() {
        assert!(playable.contains(&choice), "must play a legal column");
        return;
    }
    assert!(safe.contains(&choice), "must not allow an immediate win");
    let best = safe.iter().map(|&col| value(position, col)).max();
    assert_eq!(
        Some(value(position, choice)),
        best,
        "must pick the best balance among safe moves"
    );
}

fn audit_follow_up<G: Geometry>(position: &Position<G>, last: Option<u32>) {
    let Some(col) = last.filter(|&col| follow_up_applies(position, col)) else {
        return;
    };
    let threatened = position
        .board
        .playable()
        .any(|above| wins_at(position, above) || opponent_wins_at(position, above));
    if !threatened {
        assert_eq!(
            rules::choose(position.board, last),
            Some(col),
            "must follow up on the good row"
        );
    }
}

fn walk<G: Geometry>(
    position: &Position<G>,
    last: Option<u32>,
    seen: &mut HashSet<(G::Bits, Option<u32>)>,
    audited: &mut HashSet<G::Bits>,
) {
    if !seen.insert((position.board.key(), last)) {
        return;
    }
    if position.board.playable().next().is_none() {
        return;
    }
    if audited.insert(position.board.key()) {
        audit_strategy(position);
    }
    audit_follow_up(position, last);
    for col in position.board.playable() {
        if wins_at(position, col) {
            continue;
        }
        walk(&advance(position, col), Some(col), seen, audited);
    }
}

fn exhaustive<G: Geometry>() {
    walk(
        &start::<G>(),
        None,
        &mut HashSet::new(),
        &mut HashSet::new(),
    );
}

#[test]
fn rules_4x4_u64() {
    exhaustive::<Grid<4, 4, u64>>();
}

#[test]
fn rules_4x4_u128() {
    exhaustive::<Grid<4, 4, u128>>();
}
