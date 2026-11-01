// Pascal Pons, 7x6 test sets Test_L3_R1 to Test_L1_R3: http://blog.gamesolver.org/data/
// John Tromp, UCI Connect-4 opening database: https://archive.ics.uci.edu/dataset/26/connect+4

use std::env;
use std::error::Error;
use std::fs;
use std::num::NonZeroUsize;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use c4::board::{B76, Board, Geometry, format_moves};
use c4::dataset::parse_pons;
use c4::outcome::Outcome;
use c4::solver::Solver;

const TABLE_BITS: u32 = 21;
const SHOWN_MISMATCHES: usize = 10;
const USAGE: &str =
    "usage: verify pons <files...> | verify uci <files...> | verify uci-slice <files...>";
const BLANK: &str = "b";
const UCI_POSITIONS: usize = 67_557;
const UCI_WINS: usize = 44_473;
const UCI_LOSSES: usize = 16_635;
const UCI_DRAWS: usize = 6_449;
const PONS_POSITIONS: usize = 1_000;
const UCI_STONES: usize = 8;

#[derive(Clone, Copy)]
enum Format {
    Pons,
    Uci,
    UciSlice,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Colour {
    Cross,
    Nought,
}

impl Colour {
    const fn other(self) -> Self {
        match self {
            Self::Cross => Self::Nought,
            Self::Nought => Self::Cross,
        }
    }
}

#[derive(Clone, Copy)]
struct Request<'a> {
    format: Format,
    paths: &'a [String],
}

struct Case {
    number: usize,
    moves: String,
    board: Board<B76>,
    expected: Outcome,
}

struct Solved {
    index: usize,
    outcome: Option<Outcome>,
}

#[derive(Default)]
struct Tally {
    win: usize,
    draw: usize,
    loss: usize,
    mismatches: usize,
}

impl Tally {
    const fn count(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Win => self.win += 1,
            Outcome::Draw => self.draw += 1,
            Outcome::Loss => self.loss += 1,
        }
    }

    const fn positions(&self) -> usize {
        self.win + self.draw + self.loss
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(request) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match run(request) {
        Ok(totals) if totals.mismatches == 0 => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("verify: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(args: &[String]) -> Option<Request<'_>> {
    let (mode, paths) = args.split_first()?;
    let format = match mode.as_str() {
        "pons" => Format::Pons,
        "uci" => Format::Uci,
        "uci-slice" => Format::UciSlice,
        _ => return None,
    };
    (!paths.is_empty()).then_some(Request { format, paths })
}

fn run(request: Request<'_>) -> Result<Tally, Box<dyn Error>> {
    let started = Instant::now();
    let mut totals = Tally::default();
    for path in request.paths {
        let cases = load(request.format, path)?;
        check_file(request.format, path, &cases, &mut totals)?;
        let tally = verify_cases(&cases)?;
        show(path, &tally, started);
        totals.mismatches += tally.mismatches;
    }
    check_total(request.format, &totals)?;
    show("total", &totals, started);
    Ok(totals)
}

fn load(format: Format, path: &str) -> Result<Vec<Case>, Box<dyn Error>> {
    let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    text.lines()
        .enumerate()
        .map(|(index, line)| {
            let number = index + 1;
            let case = match format {
                Format::Pons => pons_case(number, line),
                Format::Uci | Format::UciSlice => uci_case(number, line),
            };
            case.map_err(|error| format!("{path}: {error}").into())
        })
        .collect()
}

fn check_file(
    format: Format,
    path: &str,
    cases: &[Case],
    totals: &mut Tally,
) -> Result<(), Box<dyn Error>> {
    if matches!(format, Format::Pons) && cases.len() != PONS_POSITIONS {
        return Err(format!(
            "{path}: expected {PONS_POSITIONS} positions, found {}",
            cases.len()
        )
        .into());
    }
    for case in cases {
        totals.count(case.expected);
    }
    Ok(())
}

fn check_total(format: Format, tally: &Tally) -> Result<(), Box<dyn Error>> {
    if !matches!(format, Format::Uci) {
        return Ok(());
    }
    if tally.positions() != UCI_POSITIONS {
        return Err(format!(
            "expected {UCI_POSITIONS} UCI positions, found {}",
            tally.positions()
        )
        .into());
    }
    if (tally.win, tally.loss, tally.draw) != (UCI_WINS, UCI_LOSSES, UCI_DRAWS) {
        return Err(format!(
            "expected UCI classes win {UCI_WINS}, loss {UCI_LOSSES}, draw {UCI_DRAWS}; found win {}, loss {}, draw {}",
            tally.win, tally.loss, tally.draw
        )
        .into());
    }
    Ok(())
}

fn verify_cases(cases: &[Case]) -> Result<Tally, Box<dyn Error>> {
    let outcomes = solve_all(cases)?;
    Ok(compare(cases, &outcomes))
}

fn show(label: &str, tally: &Tally, started: Instant) {
    println!(
        "{label}: {} positions (win {}, draw {}, loss {}), {} mismatches, {:.2} s",
        tally.positions(),
        tally.win,
        tally.draw,
        tally.loss,
        tally.mismatches,
        started.elapsed().as_secs_f64(),
    );
}

fn pons_case(number: usize, line: &str) -> Result<Case, Box<dyn Error>> {
    let sample = parse_pons::<B76>(line).ok_or_else(|| reject(number, "invalid Pons row"))?;
    let (moves, _) = line
        .split_once(' ')
        .ok_or_else(|| reject(number, "expected '<moves> <score>'"))?;
    Ok(Case {
        number,
        moves: String::from(moves),
        board: sample.board,
        expected: sample.result,
    })
}

fn uci_case(number: usize, line: &str) -> Result<Case, Box<dyn Error>> {
    let fields: Vec<&str> = line.split(',').collect();
    let Some((class, cells)) = fields.split_last() else {
        return Err(reject(number, "empty row"));
    };
    let cell_count = usize::try_from(B76::COLUMNS * B76::ROWS)?;
    if cells.len() != cell_count {
        return Err(reject(number, "unexpected cell count"));
    }
    let expected = match *class {
        "win" => Outcome::Win,
        "draw" => Outcome::Draw,
        "loss" => Outcome::Loss,
        _ => return Err(reject(number, "unknown class")),
    };
    let stacks = uci_stacks(cells).ok_or_else(|| reject(number, "cells are not stacks"))?;
    let stones: usize = stacks.iter().map(Vec::len).sum();
    if stones != UCI_STONES {
        return Err(reject(number, "expected 8 stones"));
    }
    let columns = play_order(&stacks).ok_or_else(|| reject(number, "no alternating move order"))?;
    let moves = format_moves(&columns);
    let board =
        Board::<B76>::from_moves(&moves).ok_or_else(|| reject(number, "not a legal position"))?;
    if !board.first_to_move() {
        return Err(reject(number, "expected x to move"));
    }
    Ok(Case {
        number,
        moves,
        board,
        expected,
    })
}

fn reject(number: usize, why: &str) -> Box<dyn Error> {
    format!("line {number}: {why}").into()
}

fn uci_stacks(cells: &[&str]) -> Option<Vec<Vec<Colour>>> {
    let rows = usize::try_from(B76::ROWS).ok()?;
    let mut stacks = Vec::new();
    for column in cells.chunks(rows) {
        let mut stack = Vec::new();
        let mut empty_seen = false;
        for cell in column {
            match *cell {
                "x" if !empty_seen => stack.push(Colour::Cross),
                "o" if !empty_seen => stack.push(Colour::Nought),
                BLANK => empty_seen = true,
                _ => return None,
            }
        }
        stacks.push(stack);
    }
    Some(stacks)
}

fn play_order(stacks: &[Vec<Colour>]) -> Option<Vec<u32>> {
    let mut heights = vec![0; stacks.len()];
    let mut columns = Vec::new();
    play_from(stacks, &mut heights, &mut columns, Colour::Cross)
}

fn play_from(
    stacks: &[Vec<Colour>],
    heights: &mut [usize],
    columns: &mut Vec<u32>,
    mover: Colour,
) -> Option<Vec<u32>> {
    if columns.len() == stacks.iter().map(Vec::len).sum::<usize>() {
        return Some(columns.clone());
    }
    for (index, stack) in stacks.iter().enumerate() {
        let height = heights[index];
        if stack.get(height) != Some(&mover) {
            continue;
        }
        let column = u32::try_from(index).ok()?;
        heights[index] += 1;
        columns.push(column);
        if let Some(order) = play_from(stacks, heights, columns, mover.other()) {
            return Some(order);
        }
        columns.pop();
        heights[index] -= 1;
    }
    None
}

fn solve_all(cases: &[Case]) -> Result<Vec<Option<Outcome>>, Box<dyn Error>> {
    let next = AtomicUsize::new(0);
    let worker_count = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let batches = thread::scope(|scope| {
        let handles: Vec<_> = (0..worker_count)
            .map(|_| scope.spawn(|| claim(cases, &next)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| "solver thread panicked"))
            .collect::<Result<Vec<_>, _>>()
    })?;
    let mut outcomes = vec![None; cases.len()];
    for solved in batches.into_iter().flatten() {
        outcomes[solved.index] = solved.outcome;
    }
    Ok(outcomes)
}

fn claim(cases: &[Case], next: &AtomicUsize) -> Vec<Solved> {
    let mut solver = Solver::<B76>::new(TABLE_BITS);
    let mut results = Vec::new();
    loop {
        let index = next.fetch_add(1, Ordering::Relaxed);
        let Some(case) = cases.get(index) else {
            return results;
        };
        results.push(Solved {
            index,
            outcome: solver.solve(case.board, None),
        });
    }
}

fn compare(cases: &[Case], outcomes: &[Option<Outcome>]) -> Tally {
    let mut tally = Tally::default();
    for (case, outcome) in cases.iter().zip(outcomes) {
        tally.count(case.expected);
        if *outcome == Some(case.expected) {
            continue;
        }
        tally.mismatches += 1;
        if tally.mismatches <= SHOWN_MISMATCHES {
            println!(
                "  line {}: {} expected {:?}, solver {:?}",
                case.number, case.moves, case.expected, outcome
            );
        }
    }
    tally
}
