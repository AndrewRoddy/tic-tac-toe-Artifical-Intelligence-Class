use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::process::ExitCode;
use std::time::Duration;

use c4::board::B87;
use c4::dataset::{Labelled, parse_line};
use c4::tournament::{Entrant, PERFECT_TABLE_BITS, Seat, Settings, run};
use c4::versus::{Summary, Tally, fixtures};

const USAGE: &str =
    "usage: versus <openings_8x7.txt> <entrant> <agent_ms> <solver_s> <workers> [limit]
entrants: safe-random, alpha-beta-handcrafted, alpha-beta-learned, perfect, mcts, allis";

struct Request {
    path: String,
    entrant: Entrant,
    per_move: Duration,
    solver_per_move: Duration,
    workers: NonZeroUsize,
    limit: Option<usize>,
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(request) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match execute(&request) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("versus: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String]) -> Option<Request> {
    let [path, name, millis, seconds, workers, rest @ ..] = args else {
        return None;
    };
    let entrant = Entrant::parse(name)?;
    let millis: u64 = millis.parse().ok().filter(|ms| *ms >= 1)?;
    let seconds: u64 = seconds.parse().ok().filter(|secs| *secs >= 1)?;
    let limit = match rest {
        [] => None,
        [limit] => Some(limit.parse().ok()?),
        _ => return None,
    };
    Some(Request {
        path: path.clone(),
        entrant,
        per_move: Duration::from_millis(millis),
        solver_per_move: Duration::from_secs(seconds),
        workers: workers.parse().ok()?,
        limit,
    })
}

fn execute(request: &Request) -> Result<(), Box<dyn Error>> {
    let mut openings = load(&request.path)?;
    if let Some(max) = request.limit {
        openings.truncate(max);
    }
    if openings.is_empty() {
        return Err("no openings in file".into());
    }
    let agent = Seat {
        entrant: request.entrant,
        per_move: request.per_move,
    };
    let fixtures = fixtures(&openings, agent, request.solver_per_move);
    let settings = Settings {
        perfect_table_bits: PERFECT_TABLE_BITS,
    };
    let mut summary = Summary::default();
    run(&fixtures, settings, request.workers.get(), |record| {
        let verdict = summary.add(&record, openings[record.opening].result);
        println!(
            "game {:>5}  opening {:>4}  {verdict:?}",
            record.game, record.opening
        );
        ControlFlow::Continue(())
    });
    print_summary(request, &summary);
    Ok(())
}

fn load(path: &str) -> Result<Vec<Labelled<B87>>, Box<dyn Error>> {
    let reader = BufReader::new(File::open(path)?);
    let mut openings = Vec::new();
    for (number, line) in reader.lines().enumerate() {
        let line = line?;
        let labelled = parse_line::<B87>(&line)
            .ok_or_else(|| format!("line {}: malformed opening", number + 1))?;
        openings.push(labelled);
    }
    Ok(openings)
}

fn print_summary(request: &Request, summary: &Summary) {
    println!(
        "entrant: {}  ms_per_move: {}  workers: {}",
        request.entrant.name(),
        request.per_move.as_millis(),
        request.workers,
    );
    println!(
        "{:<12}{:>8}{:>10}{:>22}",
        "value", "n", "kept", "95% interval"
    );
    print_row("win", summary.win);
    print_row("draw", summary.draw);
    print_row("win+draw", summary.headline());
    print_row("loss (check)", summary.loss);
    println!(
        "expired: {}  solver_errors: {}",
        summary.expired, summary.solver_errors
    );
}

fn print_row(label: &str, tally: Tally) {
    let rate = tally
        .rate()
        .map_or_else(|| "-".to_owned(), |rate| format!("{rate:.3}"));
    let interval = tally
        .interval()
        .map_or_else(|| "-".to_owned(), |interval| interval.to_string());
    println!("{label:<12}{:>8}{rate:>10}{interval:>22}", tally.games);
}
