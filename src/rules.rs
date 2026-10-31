use crate::board::{Board, Geometry};

pub fn choose<G: Geometry>(board: Board<G>, last: Option<u32>) -> Option<u32> {
    board
        .centre_order()
        .find(|&col| board.wins(col))
        .or_else(|| board.centre_order().find(|&col| board.opponent_wins(col)))
        .or_else(|| follow_up(board, last))
        .or_else(|| threat_creation(board))
        .or_else(|| board.centre_order().next())
}

fn follow_up<G: Geometry>(board: Board<G>, last: Option<u32>) -> Option<u32> {
    last.filter(|&col| board.can_play(col) && board.takes_good_row(col) && !board.allows_win(col))
}

fn balance<G: Geometry>(board: Board<G>, col: u32) -> i64 {
    let mut next = board;
    next.play(col);
    let threats = next.threat_rows();
    i64::from(threats.opponent_good) - i64::from(threats.mover_good)
}

fn own_threat_above<G: Geometry>(board: Board<G>, col: u32) -> bool {
    let mut next = board;
    next.play(col);
    next.can_play(col) && next.opponent_wins(col)
}

fn threat_creation<G: Geometry>(board: Board<G>) -> Option<u32> {
    let mut best: Option<((i64, bool), u32)> = None;
    for col in board.centre_order().filter(|&col| !board.allows_win(col)) {
        let key = (balance(board, col), !own_threat_above(board, col));
        if best.is_none_or(|(top, _)| key > top) {
            best = Some((key, col));
        }
    }
    best.map(|(_, col)| col)
}
