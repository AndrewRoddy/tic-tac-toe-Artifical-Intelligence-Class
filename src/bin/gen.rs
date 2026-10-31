use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use c4::board::{B87, Geometry, format_moves};
use c4::dataset::unique_samples;

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
    let mut out = io::stdout().lock();
    for sample in unique_samples::<B87>(request.seed, request.count, request.low, request.high) {
        writeln!(out, "{}", format_moves(&sample.columns))?;
    }
    Ok(ExitCode::SUCCESS)
}
