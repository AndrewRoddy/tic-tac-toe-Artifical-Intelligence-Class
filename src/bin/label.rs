use std::env;
use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use c4::board::{B87, B98, Board, Geometry};
use c4::outcome::Outcome;
use c4::solver::Solver;

const USAGE: &str = "usage: label <8x7|9x8> (move strings on stdin, one per line)";
const TABLE_BITS: u32 = 21;

fn run<G: Geometry>() -> io::Result<ExitCode> {
    let mut solver = Solver::<G>::new(TABLE_BITS);
    let mut out = io::stdout().lock();
    for (index, line) in io::stdin().lock().lines().enumerate() {
        let moves = line?;
        let Some(board) = Board::<G>::from_moves(&moves).filter(|_| !moves.is_empty()) else {
            eprintln!("line {}: rejected move string: {moves:?}", index + 1);
            return Ok(ExitCode::FAILURE);
        };
        let Some(label) = solver.label(board, None) else {
            eprintln!("line {}: search aborted", index + 1);
            return Ok(ExitCode::FAILURE);
        };
        let symbols: String = label
            .columns
            .iter()
            .map(|&outcome| outcome.map_or('.', Outcome::symbol))
            .collect();
        writeln!(out, "{moves} {} {symbols}", label.result.symbol())?;
        out.flush()?;
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> io::Result<ExitCode> {
    let args: Vec<String> = env::args().skip(1).collect();
    let [size] = args.as_slice() else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::FAILURE);
    };
    match size.as_str() {
        "8x7" => run::<B87>(),
        "9x8" => run::<B98>(),
        _ => {
            eprintln!("{USAGE}");
            Ok(ExitCode::FAILURE)
        }
    }
}
