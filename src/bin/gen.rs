use std::collections::HashSet;
use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use c4::board::{B87, Geometry, format_moves};
use c4::dataset::sample;
use c4::random::SplitMix;

const USAGE: &str = "usage: gen <count> <min_stones> <max_stones> <seed>";

struct Request {
    count: usize,
    low: usize,
    high: usize,
    seed: u64,
}

fn parse(args: &[String]) -> Option<Request> {
    let [count, low, high, seed] = args else {
        return None;
    };
    let low: usize = low.parse().ok()?;
    let high: usize = high.parse().ok()?;
    let cells = usize::try_from(B87::COLUMNS * B87::ROWS).ok()?;
    if !(1..=high).contains(&low) || high >= cells {
        return None;
    }
    Some(Request {
        count: count.parse().ok()?,
        low,
        high,
        seed: seed.parse().ok()?,
    })
}

fn main() -> io::Result<ExitCode> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(request) = parse(&args) else {
        eprintln!("{USAGE}");
        return Ok(ExitCode::FAILURE);
    };
    let mut rng = SplitMix::new(request.seed);
    let mut seen = HashSet::new();
    let mut out = io::stdout().lock();
    let mut targets = (request.low..=request.high).cycle().skip(1);
    let mut stones = request.low;
    while seen.len() < request.count {
        let Some(position) = sample::<B87>(&mut rng, stones) else {
            continue;
        };
        stones = targets.next().unwrap_or(request.low);
        if seen.insert(position.board.canonical()) {
            writeln!(out, "{}", format_moves(&position.columns))?;
        }
    }
    Ok(ExitCode::SUCCESS)
}
