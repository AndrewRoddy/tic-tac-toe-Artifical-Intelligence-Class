use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::process::ExitCode;
use std::time::Duration;

use c4::board::{B98, Board, Geometry, format_moves};
use c4::dataset::{Sample, unique_samples};
use c4::tournament::{
    PERFECT_TABLE_BITS, ROSTER, Rating, Record, Settings, Standing, elo_ratings, run, schedule,
    standings,
};

const USAGE: &str = "usage: tournament <openings> <min_stones> <max_stones> <seed> <ms_per_move> <workers> [out.txt]
workers: physical cores minus one";
const CELLS: u32 = B98::COLUMNS * B98::ROWS;

struct Request {
    openings: usize,
    low: usize,
    high: usize,
    seed: u64,
    per_move: Duration,
    workers: NonZeroUsize,
    path: Option<String>,
}

fn parse(args: &[String]) -> Option<Request> {
    let [openings, low, high, seed, millis, workers, rest @ ..] = args else {
        return None;
    };
    let path = match rest {
        [] => None,
        [path] => Some(path.clone()),
        _ => return None,
    };
    let openings: usize = openings.parse().ok().filter(|count| *count >= 1)?;
    let low: usize = low.parse().ok()?;
    let high: usize = high.parse().ok()?;
    let cells = usize::try_from(CELLS).ok()?;
    if !(1..=high).contains(&low) || high >= cells {
        return None;
    }
    let seed: u64 = seed.parse().ok()?;
    let millis: u64 = millis.parse().ok().filter(|ms| *ms >= 1)?;
    Some(Request {
        openings,
        low,
        high,
        seed,
        per_move: Duration::from_millis(millis),
        workers: workers.parse().ok()?,
        path,
    })
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
            eprintln!("tournament: {error}");
            ExitCode::FAILURE
        }
    }
}

fn execute(request: &Request) -> Result<(), Box<dyn Error>> {
    let openings = unique_samples::<B98>(request.seed, request.openings, request.low, request.high);
    let fixtures = schedule(&ROSTER, &openings, request.per_move);
    let settings = Settings {
        perfect_table_bits: PERFECT_TABLE_BITS,
    };
    let mut writer: Box<dyn Write> = match &request.path {
        Some(path) => Box::new(BufWriter::new(File::create(path)?)),
        None => Box::new(BufWriter::new(io::stdout().lock())),
    };
    let mut records = Vec::with_capacity(fixtures.len());
    let mut failure: Option<Box<dyn Error>> = None;
    run(&fixtures, settings, request.workers.get(), |record| {
        if let Err(error) = write_record(&mut writer, &record, &openings) {
            failure = Some(error);
            return ControlFlow::Break(());
        }
        records.push(record);
        ControlFlow::Continue(())
    })
    .ok_or("could not build network agent")?;
    if let Some(error) = failure {
        return Err(error);
    }
    writer.flush()?;
    let ratings = elo_ratings(&ROSTER, &records)
        .ok_or("Elo fit failed: roster lacks the anchor or did not converge")?;
    print_table(&standings(&ROSTER, &records), &ratings);
    Ok(())
}

fn write_record(
    writer: &mut dyn Write,
    record: &Record,
    openings: &[Sample<B98>],
) -> Result<(), Box<dyn Error>> {
    let columns: Vec<u32> = openings[record.opening]
        .columns
        .iter()
        .chain(&record.moves)
        .copied()
        .collect();
    check_legal(&columns)?;
    writeln!(
        writer,
        "{{\"opening\":\"{}\",\"first\":\"{}\",\"second\":\"{}\",\"moves\":\"{}\",\"outcome\":\"{}\"}}",
        format_moves(&openings[record.opening].columns),
        record.first.name(),
        record.second.name(),
        format_moves(&record.moves),
        record.outcome.symbol(),
    )?;
    Ok(())
}

fn check_legal(columns: &[u32]) -> Result<(), Box<dyn Error>> {
    let mut board = Board::<B98>::default();
    for &col in columns {
        if col >= B98::COLUMNS || !board.can_play(col) {
            return Err(format!("illegal column {col} in game {columns:?}").into());
        }
        board.play(col);
    }
    Ok(())
}

fn print_table(table: &[Standing], ratings: &[Rating]) {
    eprintln!(
        "{:<24}{:>7}{:>7}{:>7}{:>7}{:>8}{:>10}{:>8}",
        "entrant", "games", "wins", "draws", "losses", "points", "win rate", "elo"
    );
    for (standing, rating) in table.iter().zip(ratings) {
        debug_assert_eq!(standing.entrant, rating.entrant);
        eprintln!(
            "{:<24}{:>7}{:>7}{:>7}{:>7}{:>8.1}{:>10.3}{:>8.1}",
            standing.entrant.name(),
            standing.games,
            standing.wins,
            standing.draws,
            standing.losses,
            standing.points(),
            standing.win_rate(),
            rating.elo,
        );
    }
}
