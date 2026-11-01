use burn::module::{AutodiffModule, Module, Param};
use burn::nn::conv::{Conv2d, Conv2dConfig};
use burn::nn::{Initializer, PaddingConfig2d};
use burn::optim::{AdamConfig, GradientsParams, Optimizer};
use burn::record::{FullPrecisionSettings, NamedMpkBytesRecorder, Recorder};
use burn::tensor::activation::{log_softmax, relu, softmax};
use burn::tensor::backend::{AutodiffBackend, Backend};
use burn::tensor::{Tensor, TensorData};

use crate::accuracy::Tally;
use crate::board::{Board, Cell, Geometry};
use crate::dataset::ColumnLabelled;
use crate::outcome::Outcome;
use crate::random::SplitMix;

const KERNEL_SIZE: usize = 3;
const CLASSES: usize = 3;
const WIN_CLASS: usize = 0;
const DRAW_CLASS: usize = 1;
const LOSS_CLASS: usize = 2;
const WIN_SCORE: f32 = 1.0;
const DRAW_SCORE: f32 = 0.5;
const LOSS_SCORE: f32 = 0.0;
const MARKED: f32 = 1.0;
const UNMARKED: f32 = 0.0;
const BATCH_SIZE: usize = 256;
const UNUSED_SEED: u64 = 0;
const SHIPPED: Shape = Shape {
    width: 64,
    depth: 6,
};
const WEIGHTS: &[u8] = include_bytes!("../models/network.mpk");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub width: usize,
    pub depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Plane {
    Mover,
    Opponent,
    Playable,
}

const PLANES: [Plane; 3] = [Plane::Mover, Plane::Opponent, Plane::Playable];

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or_default()
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

fn input_shape<G: Geometry>(batch: usize) -> [usize; 4] {
    [batch, PLANES.len(), to_usize(G::ROWS), to_usize(G::COLUMNS)]
}

fn marked<G: Geometry>(plane: Plane, board: Board<G>, col: u32, row: u32) -> bool {
    let cell = board.cell(col, row);
    match plane {
        Plane::Mover => cell == Cell::Mover,
        Plane::Opponent => cell == Cell::Opponent,
        Plane::Playable => {
            cell == Cell::Empty && (row == 0 || board.cell(col, row - 1) != Cell::Empty)
        }
    }
}

fn feature<G: Geometry>(plane: Plane, board: Board<G>, col: u32, row: u32) -> f32 {
    if marked(plane, board, col, row) {
        MARKED
    } else {
        UNMARKED
    }
}

pub fn encode<G: Geometry>(board: Board<G>, values: &mut Vec<f32>) {
    for plane in PLANES {
        for row in 0..G::ROWS {
            values.extend((0..G::COLUMNS).map(|col| feature(plane, board, col, row)));
        }
    }
}

fn fan_in_bound(fan_in: usize) -> f32 {
    debug_assert!(u16::try_from(fan_in).is_ok());
    f32::from(u16::try_from(fan_in).unwrap_or(u16::MAX))
        .sqrt()
        .recip()
}

fn seeded<B: Backend, const D: usize>(
    param: Param<Tensor<B, D>>,
    bound: f32,
    rng: &mut SplitMix,
    device: &B::Device,
) -> Param<Tensor<B, D>> {
    param
        .map(|initial| {
            let shape = initial.dims();
            let count: usize = shape.iter().product();
            let values: Vec<f32> = (0..count).map(|_| rng.symmetric(bound)).collect();
            Tensor::from_data(TensorData::new(values, shape), device)
        })
        .set_require_grad(true)
}

fn same(inputs: usize, outputs: usize) -> Conv2dConfig {
    Conv2dConfig::new([inputs, outputs], [KERNEL_SIZE; 2]).with_padding(PaddingConfig2d::Same)
}

fn conv<B: Backend>(config: Conv2dConfig, rng: &mut SplitMix, device: &B::Device) -> Conv2d<B> {
    let [inputs, _] = config.channels;
    let [height, width] = config.kernel_size;
    let mut layer = config.with_initializer(Initializer::Zeros).init(device);
    let bound = fan_in_bound(inputs * height * width);
    layer.weight = seeded(layer.weight, bound, rng, device);
    layer.bias = layer.bias.map(|bias| seeded(bias, bound, rng, device));
    layer
}

#[derive(Module, Debug)]
pub struct Net<B: Backend> {
    stem: Conv2d<B>,
    body: Vec<Conv2d<B>>,
    head: Conv2d<B>,
}

impl<B: Backend> Net<B> {
    #[must_use]
    pub fn new(shape: Shape, rng: &mut SplitMix, device: &B::Device) -> Self {
        let stem = conv(same(PLANES.len(), shape.width), rng, device);
        let body = (1..shape.depth)
            .map(|_| conv(same(shape.width, shape.width), rng, device))
            .collect();
        let head = conv(
            Conv2dConfig::new([shape.width, CLASSES], [1; 2]).with_padding(PaddingConfig2d::Valid),
            rng,
            device,
        );
        Self { stem, body, head }
    }

    fn logits(&self, input: Tensor<B, 4>) -> Tensor<B, 3> {
        let [batch, _, _, columns] = input.dims();
        let mut hidden = relu(self.stem.forward(input));
        for layer in &self.body {
            hidden = relu(layer.forward(hidden));
        }
        self.head
            .forward(hidden)
            .max_dim(2)
            .reshape([batch, CLASSES, columns])
            .swap_dims(1, 2)
    }

    #[must_use]
    pub fn forecast<G: Geometry>(&self, board: Board<G>, device: &B::Device) -> Option<Forecast> {
        let mut values = Vec::new();
        encode(board, &mut values);
        let input = Tensor::<B, 4>::from_data(TensorData::new(values, input_shape::<G>(1)), device);
        let probabilities = softmax(self.logits(input), 2)
            .into_data()
            .to_vec::<f32>()
            .ok()?;
        let forecast = forecast_from(&probabilities)?;
        (forecast.columns.len() == to_usize(G::COLUMNS)).then_some(forecast)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Odds {
    pub win: f32,
    pub draw: f32,
    pub loss: f32,
}

impl Odds {
    fn expected(self) -> f32 {
        self.win.mul_add(
            WIN_SCORE,
            self.draw.mul_add(DRAW_SCORE, self.loss * LOSS_SCORE),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub columns: Vec<Odds>,
}

fn forecast_from(probabilities: &[f32]) -> Option<Forecast> {
    let columns = probabilities
        .chunks_exact(CLASSES)
        .map(|probs| {
            Some(Odds {
                win: *probs.get(WIN_CLASS)?,
                draw: *probs.get(DRAW_CLASS)?,
                loss: *probs.get(LOSS_CLASS)?,
            })
        })
        .collect::<Option<Vec<Odds>>>()?;
    Some(Forecast { columns })
}

#[must_use]
pub fn choose<G: Geometry>(board: Board<G>, forecast: &Forecast) -> Option<u32> {
    let mut best: Option<(u32, f32)> = None;
    for col in board.centre_order() {
        let Some(odds) = forecast.columns.get(to_usize(col)) else {
            continue;
        };
        let value = odds.expected();
        if best.is_none_or(|(_, top)| value > top) {
            best = Some((col, value));
        }
    }
    best.map(|(col, _)| col)
}

const fn class_of(outcome: Outcome) -> usize {
    match outcome {
        Outcome::Win => WIN_CLASS,
        Outcome::Draw => DRAW_CLASS,
        Outcome::Loss => LOSS_CLASS,
    }
}

fn legal_columns<G: Geometry>(example: &ColumnLabelled<G>) -> u32 {
    to_u32(example.columns.iter().flatten().count())
}

fn target(label: Option<Outcome>, class: usize) -> f32 {
    if label.map(class_of) == Some(class) {
        MARKED
    } else {
        UNMARKED
    }
}

fn push_labels<G: Geometry>(
    example: &ColumnLabelled<G>,
    targets: &mut Vec<f32>,
    legal: &mut Vec<f32>,
) {
    for label in &example.columns {
        targets.extend((0..CLASSES).map(|class| target(*label, class)));
        legal.push(if label.is_some() { MARKED } else { UNMARKED });
    }
}

struct Batch<B: Backend> {
    input: Tensor<B, 4>,
    targets: Tensor<B, 3>,
    legal: Tensor<B, 2>,
}

fn batch<B: Backend, G: Geometry>(items: &[&ColumnLabelled<G>], device: &B::Device) -> Batch<B> {
    let mut inputs = Vec::new();
    let mut targets = Vec::new();
    let mut legal = Vec::new();
    for example in items {
        encode(example.position.board, &mut inputs);
        push_labels(example, &mut targets, &mut legal);
    }
    let count = items.len();
    let columns = to_usize(G::COLUMNS);
    Batch {
        input: Tensor::from_data(TensorData::new(inputs, input_shape::<G>(count)), device),
        targets: Tensor::from_data(TensorData::new(targets, [count, columns, CLASSES]), device),
        legal: Tensor::from_data(TensorData::new(legal, [count, columns]), device),
    }
}

fn nll_sum<B: Backend>(logits: Tensor<B, 3>, targets: Tensor<B, 3>) -> Tensor<B, 1> {
    (log_softmax(logits, 2) * targets).sum().neg()
}

fn scalar<B: Backend>(tensor: Tensor<B, 1>) -> Option<f64> {
    tensor
        .into_data()
        .to_vec::<f32>()
        .ok()?
        .first()
        .map(|&value| f64::from(value))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub loss: f64,
    pub tally: Tally,
}

pub fn evaluate<B: Backend, G: Geometry>(
    net: &Net<B>,
    examples: &[ColumnLabelled<G>],
    device: &B::Device,
) -> Option<Evaluation> {
    let mut nll_total = 0.0;
    let mut legal_total = 0;
    let mut tally = Tally::default();
    for chunk in examples.chunks(BATCH_SIZE) {
        let items: Vec<&ColumnLabelled<G>> = chunk.iter().collect();
        let prepared = batch::<B, G>(&items, device);
        let logits = net.logits(prepared.input);
        nll_total += scalar(nll_sum(logits.clone(), prepared.targets))?;
        let probabilities = softmax(logits, 2).into_data().to_vec::<f32>().ok()?;
        let width = to_usize(G::COLUMNS) * CLASSES;
        for (&example, probs) in items.iter().zip(probabilities.chunks_exact(width)) {
            let forecast = forecast_from(probs)?;
            let column = choose(example.position.board, &forecast)?;
            tally.record(&example.columns, column)?;
            legal_total += legal_columns(example);
        }
    }
    Some(Evaluation {
        loss: nll_total / f64::from(legal_total),
        tally,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Schedule {
    pub epochs: usize,
    pub learning_rate: f64,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpochReport {
    pub epoch: usize,
    pub train_loss: f64,
    pub watched_loss: f64,
    pub watched: Tally,
}

#[derive(Debug)]
pub struct Trained<B: Backend> {
    pub net: Net<B>,
    pub reports: Vec<EpochReport>,
}

#[derive(Clone, Copy)]
pub struct Splits<'a, G: Geometry> {
    pub training: &'a [ColumnLabelled<G>],
    pub watch: &'a [ColumnLabelled<G>],
}

pub fn train<B: AutodiffBackend, G: Geometry>(
    shape: Shape,
    splits: Splits<'_, G>,
    schedule: Schedule,
    device: &B::Device,
) -> Option<Trained<B::InnerBackend>> {
    let Splits { training, watch } = splits;
    let mut rng = SplitMix::new(schedule.seed);
    let mut model = Net::<B>::new(shape, &mut rng, device);
    let mut optimizer = AdamConfig::new().init::<B, Net<B>>();
    let mut order: Vec<usize> = (0..training.len()).collect();
    let mut reports = Vec::with_capacity(schedule.epochs);
    for epoch in 0..schedule.epochs {
        rng.shuffle(&mut order);
        let mut train_nll = 0.0;
        let mut train_legal = 0;
        for chunk in order.chunks(BATCH_SIZE) {
            let items: Vec<&ColumnLabelled<G>> = chunk
                .iter()
                .filter_map(|&index| training.get(index))
                .collect();
            train_legal += items
                .iter()
                .map(|example| legal_columns(example))
                .sum::<u32>();
            let prepared = batch::<B, G>(&items, device);
            let nll = nll_sum(model.logits(prepared.input), prepared.targets);
            let legal = prepared.legal.sum();
            train_nll += scalar(nll.clone())?;
            let grads = GradientsParams::from_grads((nll / legal).backward(), &model);
            model = optimizer.step(schedule.learning_rate, model, grads);
        }
        let watched = evaluate(&model.valid(), watch, device)?;
        reports.push(EpochReport {
            epoch,
            train_loss: train_nll / f64::from(train_legal),
            watched_loss: watched.loss,
            watched: watched.tally,
        });
    }
    Some(Trained {
        net: model.valid(),
        reports,
    })
}

#[must_use]
pub fn shipped<B: Backend>(device: &B::Device) -> Option<Net<B>> {
    from_weights(SHIPPED, WEIGHTS, device)
}

#[must_use]
pub fn from_weights<B: Backend>(shape: Shape, bytes: &[u8], device: &B::Device) -> Option<Net<B>> {
    let recorder = NamedMpkBytesRecorder::<FullPrecisionSettings>::new();
    let record =
        Recorder::<B>::load::<<Net<B> as Module<B>>::Record>(&recorder, bytes.to_vec(), device)
            .ok()?;
    let mut rng = SplitMix::new(UNUSED_SEED);
    Some(Net::new(shape, &mut rng, device).load_record(record))
}

#[must_use]
pub fn to_weights<B: Backend>(net: Net<B>) -> Option<Vec<u8>> {
    let recorder = NamedMpkBytesRecorder::<FullPrecisionSettings>::new();
    Recorder::<B>::record(&recorder, net.into_record(), ()).ok()
}
