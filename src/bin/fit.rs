use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::ExitCode;

use c4::board::B87;
use c4::dataset::{Labelled, parse_line, split};
use c4::learn::{Model, Row, constant_row, evaluate, fit, handcrafted_row, learned_row};
use c4::outcome::Outcome;

const DATA: &str = "data/train_8x7.txt";
const SCALE: f64 = 1_000.0;

fn main() -> ExitCode {
    match run(DATA) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fit failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(path: &str) -> Result<(), Box<dyn Error>> {
    let samples = read_samples(path)?;
    let parts = split(samples);
    let (train, held) = (parts.train.as_slice(), parts.held.as_slice());
    if held.is_empty() {
        return Err("data has no held-out lines".into());
    }
    let wins = train
        .iter()
        .filter(|sample| sample.result == Outcome::Win)
        .count();
    println!(
        "train {} (wins {wins}), held-out {}",
        train.len(),
        held.len()
    );
    fit_and_print(
        "constant",
        &rows(train, constant_row),
        &rows(held, constant_row),
    )?;
    fit_and_print(
        "handcrafted 4/1 score, calibrated",
        &rows(train, handcrafted_row),
        &rows(held, handcrafted_row),
    )?;
    let learned_train = rows(train, learned_row);
    let learned_held = rows(held, learned_row);
    let learned = fit_and_print("learned", &learned_train, &learned_held)?;
    let scaled: Vec<f64> = learned
        .weights
        .iter()
        .map(|weight| (weight * SCALE).round())
        .collect();
    println!("shipped integers (x{SCALE}): {scaled:?}");
    let shipped = Model {
        weights: scaled.iter().map(|weight| weight / SCALE).collect(),
    };
    print_model("shipped (rounded)", &shipped, &learned_train, &learned_held);
    Ok(())
}

fn read_samples(path: &str) -> Result<Vec<Labelled<B87>>, Box<dyn Error>> {
    let mut samples = Vec::new();
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        samples.push(parse_line(&line).ok_or_else(|| format!("malformed line: {line}"))?);
    }
    Ok(samples)
}

fn rows(samples: &[Labelled<B87>], row: fn(&Labelled<B87>) -> Row) -> Vec<Row> {
    samples.iter().map(row).collect()
}

fn fit_and_print(name: &str, train: &[Row], held: &[Row]) -> Result<Model, Box<dyn Error>> {
    let model = fit(train).ok_or_else(|| format!("{name} fit did not converge"))?;
    print_model(name, &model, train, held);
    Ok(model)
}

fn print_model(name: &str, model: &Model, train: &[Row], held: &[Row]) {
    let fitted = evaluate(model, train);
    let tested = evaluate(model, held);
    println!("{name}: weights {:?}", model.weights);
    println!(
        "  train log-loss {:.6} accuracy {:.4}",
        fitted.log_loss, fitted.accuracy
    );
    println!(
        "  held-out log-loss {:.6} accuracy {:.4}",
        tested.log_loss, tested.accuracy
    );
}
