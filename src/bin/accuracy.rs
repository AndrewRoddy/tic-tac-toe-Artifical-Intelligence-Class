use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::num::NonZeroUsize;
use std::ops::{ControlFlow, RangeInclusive};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use c4::accuracy::Tally;
use c4::board::{B87, B98, Geometry};
use c4::dataset::{ColumnLabelled, parse_column_labelled, split};
use c4::outcome::Outcome;
use c4::parallel::map_each;
use c4::tournament::{Entrant, PERFECT_TABLE_BITS, ROSTER};

const USAGE: &str =
    "usage: accuracy <8x7|9x8> <labelled.txt> <held-out|all> <entrant> <ms_per_move> <workers> [limit]
entrants: safe-random, alpha-beta-handcrafted, alpha-beta-learned, perfect, mcts, allis";
const PHASES: [RangeInclusive<u32>; 10] = [
    10..=12,
    14..=15,
    16..=20,
    21..=26,
    27..=32,
    33..=38,
    39..=44,
    45..=50,
    51..=56,
    57..=60,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Set {
    HeldOut,
    All,
}

impl Set {
    fn parse(text: &str) -> Option<Self> {
        [Self::HeldOut, Self::All]
            .into_iter()
            .find(|set| set.name() == text)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::HeldOut => "held-out",
            Self::All => "all",
        }
    }
}

struct Request {
    path: String,
    set: Set,
    entrant: Entrant,
    per_move: Duration,
    workers: NonZeroUsize,
    limit: Option<usize>,
}

struct Position<G: Geometry> {
    seed: u64,
    labelled: ColumnLabelled<G>,
}

struct Answer {
    columns: Vec<Option<Outcome>>,
    stones: u32,
    column: u32,
}

struct Report {
    total: Tally,
    phases: Vec<Tally>,
}

impl Report {
    fn new() -> Self {
        Self {
            total: Tally::default(),
            phases: vec![Tally::default(); PHASES.len()],
        }
    }

    fn record(&mut self, answer: &Answer) -> Option<()> {
        self.total.record(&answer.columns, answer.column)?;
        let phase = PHASES
            .iter()
            .position(|phase| phase.contains(&answer.stones));
        if let Some(tally) = phase.and_then(|index| self.phases.get_mut(index)) {
            tally.record(&answer.columns, answer.column)?;
        }
        Some(())
    }
}

fn parse(args: &[String]) -> Option<Request> {
    let [path, set, name, millis, workers, rest @ ..] = args else {
        return None;
    };
    let set = Set::parse(set)?;
    let entrant = ROSTER
        .iter()
        .copied()
        .find(|entrant| entrant.name() == name.as_str())?;
    let millis: u64 = millis.parse().ok().filter(|ms| *ms >= 1)?;
    let limit = match rest {
        [] => None,
        [limit] => Some(limit.parse::<usize>().ok().filter(|count| *count >= 1)?),
        _ => return None,
    };
    Some(Request {
        path: path.clone(),
        set,
        entrant,
        per_move: Duration::from_millis(millis),
        workers: workers.parse().ok()?,
        limit,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [size, rest @ ..] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match size.as_str() {
        "8x7" => run::<B87>(rest),
        "9x8" => run::<B98>(rest),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run<G: Geometry + 'static>(args: &[String]) -> ExitCode {
    let Some(request) = parse(args) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match execute::<G>(&request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("accuracy: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute<G: Geometry + 'static>(request: &Request) -> Result<(), Box<dyn Error>> {
    let mut positions = load::<G>(&request.path, request.set)?;
    if let Some(max) = request.limit {
        positions.truncate(max);
    }
    if positions.is_empty() {
        return Err("no positions in selected set".into());
    }
    let mut report = Report::new();
    let mut failed = false;
    map_each(
        &positions,
        request.workers.get(),
        |position| answer::<G>(position, request.entrant, request.per_move),
        |answer| {
            if report.record(&answer).is_some() {
                ControlFlow::Continue(())
            } else {
                failed = true;
                ControlFlow::Break(())
            }
        },
    );
    if failed {
        return Err("position without a legal labelled column".into());
    }
    print_report(request, &report);
    Ok(())
}

fn load<G: Geometry>(path: &str, set: Set) -> Result<Vec<Position<G>>, Box<dyn Error>> {
    let reader = BufReader::new(File::open(path)?);
    let mut lines = Vec::new();
    for (line, seed) in reader.lines().zip(0_u64..) {
        let line = line?;
        let labelled = parse_column_labelled::<G>(&line)
            .ok_or_else(|| format!("line {}: malformed labelled position", seed + 1))?;
        lines.push(Position { seed, labelled });
    }
    Ok(match set {
        Set::HeldOut => split(lines).held,
        Set::All => lines,
    })
}

fn answer<G: Geometry + 'static>(
    position: &Position<G>,
    entrant: Entrant,
    per_move: Duration,
) -> Answer {
    let board = position.labelled.position.board;
    let mut agent = entrant.build::<G>(position.seed, PERFECT_TABLE_BITS);
    let column = agent.choose(board, Instant::now() + per_move);
    Answer {
        columns: position.labelled.columns.clone(),
        stones: G::COLUMNS * G::ROWS - board.empty_cells(),
        column,
    }
}

fn print_report(request: &Request, report: &Report) {
    println!(
        "file: {}  set: {}  entrant: {}  ms_per_move: {}  workers: {}",
        request.path,
        request.set.name(),
        request.entrant.name(),
        request.per_move.as_millis(),
        request.workers,
    );
    println!(
        "{:<8}{:>8}{:>10}{:>22}{:>10}",
        "phase", "n", "accuracy", "95% interval", "baseline"
    );
    for (range, tally) in PHASES.iter().zip(&report.phases) {
        print_row(&format!("{}-{}", range.start(), range.end()), tally);
    }
    print_row("total", &report.total);
}

fn print_row(label: &str, tally: &Tally) {
    let interval = tally.interval().map_or_else(
        || "-".to_owned(),
        |interval| format!("[{:.3}, {:.3}]", interval.low, interval.high),
    );
    println!(
        "{:<8}{:>8}{:>10}{:>22}{:>10}",
        label,
        tally.positions(),
        fraction(tally.accuracy()),
        interval,
        fraction(tally.baseline()),
    );
}

fn fraction(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.3}"))
}
