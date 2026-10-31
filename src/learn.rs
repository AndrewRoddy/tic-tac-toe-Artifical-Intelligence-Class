use crate::board::Geometry;
use crate::dataset::Labelled;
use crate::outcome::Outcome;
use crate::search::{Features, Weights};

const MAX_ITERATIONS: u32 = 50;
const STEP_TOLERANCE: f64 = 1e-9;

pub struct Row {
    pub inputs: Vec<f64>,
    pub outcome: Outcome,
}

pub struct Model {
    pub weights: Vec<f64>,
}

impl Model {
    fn logit(&self, inputs: &[f64]) -> f64 {
        dot(&self.weights, inputs)
    }
}

#[must_use]
pub fn constant_row<G: Geometry>(sample: &Labelled<G>) -> Row {
    Row {
        inputs: vec![1.0],
        outcome: sample.result,
    }
}

#[must_use]
pub fn handcrafted_row<G: Geometry>(sample: &Labelled<G>) -> Row {
    let score = dot(&Weights::DEFAULT.0.map(f64::from), &features(sample));
    Row {
        inputs: vec![score, 1.0],
        outcome: sample.result,
    }
}

#[must_use]
pub fn learned_row<G: Geometry>(sample: &Labelled<G>) -> Row {
    let [mover_good, mover_other, opponent_good, opponent_other] = features(sample);
    Row {
        inputs: vec![mover_good, mover_other, opponent_good, opponent_other, 1.0],
        outcome: sample.result,
    }
}

fn features<G: Geometry>(sample: &Labelled<G>) -> [f64; 4] {
    Features::of(sample.board).0.map(f64::from)
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(weight, input)| weight * input)
        .sum()
}

struct Derivatives {
    gradient: Vec<f64>,
    hessian: Vec<Vec<f64>>,
}

#[must_use]
pub fn fit(rows: &[Row]) -> Option<Model> {
    let width = rows.first().map_or(0, |row| row.inputs.len());
    let mut weights = vec![0.0; width];
    for _ in 0..MAX_ITERATIONS {
        let totals = derivatives(rows, &weights);
        let step = solve(totals.hessian, totals.gradient)?;
        for (weight, delta) in weights.iter_mut().zip(&step) {
            *weight += delta;
        }
        if step.iter().all(|delta| delta.abs() < STEP_TOLERANCE) {
            return Some(Model { weights });
        }
    }
    None
}

fn derivatives(rows: &[Row], weights: &[f64]) -> Derivatives {
    let mut totals = Derivatives {
        gradient: vec![0.0; weights.len()],
        hessian: vec![vec![0.0; weights.len()]; weights.len()],
    };
    for row in rows {
        let probability = sigmoid(dot(weights, &row.inputs));
        let residual = target(row.outcome) - probability;
        let curvature = probability * (1.0 - probability);
        accumulate(&mut totals, &row.inputs, residual, curvature);
    }
    totals
}

const fn target(outcome: Outcome) -> f64 {
    match outcome {
        Outcome::Win => 1.0,
        Outcome::Draw => 0.5,
        Outcome::Loss => 0.0,
    }
}

fn accumulate(totals: &mut Derivatives, inputs: &[f64], residual: f64, curvature: f64) {
    for (slot, input) in totals.gradient.iter_mut().zip(inputs) {
        *slot += residual * input;
    }
    for (hessian_row, outer) in totals.hessian.iter_mut().zip(inputs) {
        for (cell, inner) in hessian_row.iter_mut().zip(inputs) {
            *cell += curvature * outer * inner;
        }
    }
}

fn sigmoid(logit: f64) -> f64 {
    1.0 / (1.0 + (-logit).exp())
}

fn solve(mut matrix: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let width = rhs.len();
    for column in 0..width {
        let pivot = (column..width).max_by(|&left, &right| {
            matrix[left][column]
                .abs()
                .total_cmp(&matrix[right][column].abs())
        })?;
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        if !matrix[column][column].is_normal() {
            return None;
        }
        eliminate(&mut matrix, &mut rhs, column);
    }
    let mut solution = vec![0.0; width];
    for row in (0..width).rev() {
        let tail: f64 = (row + 1..width)
            .map(|inner| matrix[row][inner] * solution[inner])
            .sum();
        solution[row] = (rhs[row] - tail) / matrix[row][row];
    }
    Some(solution)
}

fn eliminate(matrix: &mut [Vec<f64>], rhs: &mut [f64], column: usize) {
    let (above, below) = matrix.split_at_mut(column + 1);
    let pivot_row = &above[column];
    let (rhs_above, rhs_below) = rhs.split_at_mut(column + 1);
    let pivot_rhs = rhs_above[column];
    for (row, rhs_cell) in below.iter_mut().zip(rhs_below.iter_mut()) {
        let factor = row[column] / pivot_row[column];
        for (cell, pivot_cell) in row.iter_mut().zip(pivot_row).skip(column) {
            *cell -= factor * pivot_cell;
        }
        *rhs_cell -= factor * pivot_rhs;
    }
}

pub struct Metrics {
    pub log_loss: f64,
    pub accuracy: f64,
}

#[must_use]
pub fn evaluate(model: &Model, rows: &[Row]) -> Metrics {
    let mut loss_total = 0.0;
    let mut correct = 0.0;
    let mut count = 0.0;
    for row in rows {
        let logit = model.logit(&row.inputs);
        loss_total += log_loss(logit, target(row.outcome));
        if predicted_correctly(logit, row.outcome) {
            correct += 1.0;
        }
        count += 1.0;
    }
    Metrics {
        log_loss: loss_total / count,
        accuracy: correct / count,
    }
}

fn log_loss(logit: f64, expected: f64) -> f64 {
    expected.mul_add(-logit, softplus(logit))
}

fn softplus(value: f64) -> f64 {
    value.max(0.0) + (-value.abs()).exp().ln_1p()
}

fn predicted_correctly(logit: f64, outcome: Outcome) -> bool {
    match outcome {
        Outcome::Win => logit > 0.0,
        Outcome::Draw | Outcome::Loss => logit <= 0.0,
    }
}
