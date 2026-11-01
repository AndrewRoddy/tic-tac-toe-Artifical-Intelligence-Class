use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufReader};

use c4::board::B87;
use c4::dataset::{Labelled, parse_line, split};
use c4::learn::{Model, Row, constant_row, evaluate, fit, handcrafted_row, learned_row};
use c4::outcome::Outcome;
use c4::search::Weights;

const DATA: &str = "data/train_8x7.txt";
const DATA_LINES: usize = 106_898;
const FIT_LINES: usize = 2_000;
const SCALE: f64 = 1_000.0;
const INTERCEPT: f64 = 0.972;

fn slice(start: usize, count: usize) -> Result<Vec<Labelled<B87>>, Box<dyn Error>> {
    let mut samples = Vec::with_capacity(count);
    for line in BufReader::new(File::open(DATA)?)
        .lines()
        .skip(start)
        .take(count)
    {
        let line = line?;
        samples.push(parse_line(&line).ok_or_else(|| format!("malformed line: {line}"))?);
    }
    if samples.len() != count {
        return Err(format!("{DATA} ends before line {}", start + count).into());
    }
    Ok(samples)
}

fn rows(samples: &[Labelled<B87>], row: fn(&Labelled<B87>) -> Row) -> Vec<Row> {
    samples.iter().map(row).collect()
}

fn shipped() -> Model {
    let [mover_good, mover_other, opponent_good, opponent_other] =
        Weights::LEARNED.0.map(|weight| f64::from(weight) / SCALE);
    Model {
        weights: vec![
            mover_good,
            mover_other,
            opponent_good,
            opponent_other,
            INTERCEPT,
        ],
    }
}

#[test]
fn shipped_beats_handcrafted() -> Result<(), Box<dyn Error>> {
    let parts = split(slice(0, DATA_LINES)?);
    let (train, held) = (parts.train, parts.held);
    let handcrafted =
        fit(&rows(&train, handcrafted_row)).ok_or("handcrafted fit did not converge")?;
    let baseline = evaluate(&handcrafted, &rows(&held, handcrafted_row)).log_loss;
    let shipped_loss = evaluate(&shipped(), &rows(&held, learned_row)).log_loss;
    assert!(
        shipped_loss < baseline,
        "shipped {shipped_loss} handcrafted {baseline}"
    );
    Ok(())
}

#[test]
fn constant_fit_matches_mean() -> Result<(), Box<dyn Error>> {
    let samples = slice(0, FIT_LINES)?;
    let model = fit(&rows(&samples, constant_row)).ok_or("constant fit did not converge")?;
    let count = |outcome: Outcome| {
        samples
            .iter()
            .filter(|sample| sample.result == outcome)
            .count()
    };
    let wins = f64::from(u32::try_from(count(Outcome::Win))?);
    let draws = f64::from(u32::try_from(count(Outcome::Draw))?);
    let total = f64::from(u32::try_from(samples.len())?);
    let score = wins + draws / 2.0;
    let expected = (score / (total - score)).ln();
    assert!(
        (model.weights[0] - expected).abs() < 1e-9,
        "fit {} expected {expected}",
        model.weights[0]
    );
    Ok(())
}
