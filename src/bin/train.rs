use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use burn::backend::flex::FlexDevice;
use burn::backend::{Autodiff, Flex};
use c4::board::B87;
use c4::dataset::{self, ColumnLabelled, parse_column_labelled};
use c4::net::{EpochReport, Schedule, Shape, Splits, to_weights, train};

const USAGE: &str = "usage: train <sweep|final> <width> <depth> <epochs> <learning_rate> [out]";
const DATA_PATH: &str = "data/train_8x7.txt";
const DEFAULT_OUT: &str = "models/network";
const SEED: u64 = 1;

type Train = Autodiff<Flex>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Sweep,
    Final,
}

struct Request {
    mode: Mode,
    shape: Shape,
    schedule: Schedule,
    out: PathBuf,
}

fn parse(args: &[String]) -> Option<Request> {
    let [mode, width, depth, epochs, rate, rest @ ..] = args else {
        return None;
    };
    let mode = match mode.as_str() {
        "sweep" => Mode::Sweep,
        "final" => Mode::Final,
        _ => return None,
    };
    let out = rest
        .first()
        .map_or_else(|| PathBuf::from(DEFAULT_OUT), PathBuf::from);
    Some(Request {
        mode,
        shape: Shape {
            width: width.parse().ok()?,
            depth: depth.parse().ok()?,
        },
        schedule: Schedule {
            epochs: epochs.parse().ok()?,
            learning_rate: rate.parse().ok()?,
            seed: SEED,
        },
        out,
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
            eprintln!("train: {error}");
            ExitCode::FAILURE
        }
    }
}

fn load_positions(path: &str) -> Result<Vec<ColumnLabelled<B87>>, Box<dyn Error>> {
    let text = fs::read_to_string(path)?;
    let positions = text
        .lines()
        .enumerate()
        .map(|(index, line)| {
            parse_column_labelled::<B87>(line).ok_or_else(|| {
                Box::<dyn Error>::from(format!("line {}: invalid labelled position", index + 1))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(positions)
}

fn execute(request: &Request) -> Result<(), Box<dyn Error>> {
    let parts = dataset::split(load_positions(DATA_PATH)?);
    let (training, watch) = match request.mode {
        Mode::Sweep => {
            let inner = dataset::split(parts.train);
            (inner.train, inner.held)
        }
        Mode::Final => (parts.train, Vec::new()),
    };
    let device = FlexDevice;
    let started = Instant::now();
    let trained = train::<Train, B87>(
        request.shape,
        Splits {
            training: &training,
            watch: &watch,
        },
        request.schedule,
        &device,
    )
    .ok_or("training failed")?;
    println!("training time {:.1} s", started.elapsed().as_secs_f64());
    print_epochs(&trained.reports, request.mode);
    if request.mode == Mode::Final {
        let bytes = to_weights(trained.net).ok_or("could not serialise model")?;
        let path = request.out.with_extension("mpk");
        fs::write(&path, &bytes)?;
        println!("model {} bytes to {}", bytes.len(), path.display());
    }
    Ok(())
}

fn print_epochs(reports: &[EpochReport], mode: Mode) {
    for report in reports {
        match mode {
            Mode::Sweep => println!(
                "epoch {:>2}  train loss {:.4}  watched loss {:.4}  accuracy {}  baseline {}",
                report.epoch,
                report.train_loss,
                report.watched_loss,
                fraction(report.watched.accuracy()),
                fraction(report.watched.baseline()),
            ),
            Mode::Final => println!(
                "epoch {:>2}  train loss {:.4}",
                report.epoch, report.train_loss
            ),
        }
    }
}

fn fraction(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.3}"))
}
