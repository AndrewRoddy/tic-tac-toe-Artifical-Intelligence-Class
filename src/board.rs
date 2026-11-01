use std::marker::PhantomData;

use crate::bits::Bits;

pub trait Geometry: Copy {
    const COLUMNS: u32;
    const ROWS: u32;
    type Bits: Bits;
}

pub const COLUMN_LIMIT: usize = 9;

const fn columns_within(columns: u32) -> bool {
    let mut count = 0;
    let mut left = 0;
    while left < columns {
        count += 1;
        left += 1;
    }
    count <= COLUMN_LIMIT
}

#[derive(Clone, Copy)]
pub struct Grid<const W: u32, const H: u32, B>(PhantomData<B>);

impl<const W: u32, const H: u32, B: Bits> Geometry for Grid<W, H, B> {
    const COLUMNS: u32 = W;
    const ROWS: u32 = H;
    type Bits = B;
}

pub type B76 = Grid<7, 6, u64>;
pub type B87 = Grid<8, 7, u64>;
pub type B98 = Grid<9, 8, u128>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Threats {
    pub mover_good: u32,
    pub mover_other: u32,
    pub opponent_good: u32,
    pub opponent_other: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Mover,
    Opponent,
}

#[derive(Clone, Copy)]
pub struct Board<G: Geometry> {
    current: G::Bits,
    mask: G::Bits,
}

impl<G: Geometry> Default for Board<G> {
    fn default() -> Self {
        let () = Self::FITS;
        Self {
            current: G::Bits::ZERO,
            mask: G::Bits::ZERO,
        }
    }
}

impl<G: Geometry> Board<G> {
    const STRIDE: u32 = G::ROWS + 1;
    const FITS: () = {
        assert!(
            G::COLUMNS * (G::ROWS + 1) <= G::Bits::BITS,
            "board does not fit its bit type"
        );
        assert!(columns_within(G::COLUMNS), "too many columns");
    };

    pub fn can_play(&self, col: u32) -> bool {
        self.mask & Self::top(col) == G::Bits::ZERO
    }

    pub fn playable(self) -> impl Iterator<Item = u32> {
        (0..G::COLUMNS).filter(move |&col| self.can_play(col))
    }

    pub fn allows_win(&self, col: u32) -> bool {
        let mut next = *self;
        next.play(col);
        next.can_win()
    }

    pub fn takes_good_row(&self, col: u32) -> bool {
        let row = (self.mask & Self::column(col)).count_ones();
        Self::first_takes_row(row) == self.first_to_move()
    }

    pub fn centre_order(self) -> impl Iterator<Item = u32> {
        (0..G::COLUMNS)
            .map(centre_first::<G>)
            .filter(move |&col| self.can_play(col))
    }

    pub fn wins(&self, col: u32) -> bool {
        debug_assert!(self.can_play(col));
        Self::winning_cells(self.current, self.mask) & self.spot(col) != G::Bits::ZERO
    }

    pub fn opponent_wins(&self, col: u32) -> bool {
        debug_assert!(self.can_play(col));
        Self::winning_cells(self.mask ^ self.current, self.mask) & self.spot(col) != G::Bits::ZERO
    }

    pub fn can_win(&self) -> bool {
        Self::winning_cells(self.current, self.mask) & self.playable_cells() != G::Bits::ZERO
    }

    pub fn threat_count_after(&self, col: u32) -> u32 {
        let mut next = *self;
        next.play(col);
        Self::winning_cells(next.mask ^ next.current, next.mask).count_ones()
    }

    pub fn first_to_move(&self) -> bool {
        self.mask.count_ones().is_multiple_of(2)
    }

    pub fn empty_cells(&self) -> u32 {
        G::COLUMNS * G::ROWS - self.mask.count_ones()
    }

    pub fn cell(&self, col: u32, row: u32) -> Cell {
        debug_assert!(col < G::COLUMNS && row < G::ROWS);
        let bit = Self::bottom(col) << row;
        if self.mask & bit == G::Bits::ZERO {
            Cell::Empty
        } else if self.current & bit == G::Bits::ZERO {
            Cell::Opponent
        } else {
            Cell::Mover
        }
    }

    pub fn threat_rows(&self) -> Threats {
        let mover = Self::winning_cells(self.current, self.mask);
        let waiting = Self::winning_cells(self.mask ^ self.current, self.mask);
        let good = if self.first_to_move() {
            Self::first_rows()
        } else {
            !Self::first_rows()
        };
        Threats {
            mover_good: (mover & good).count_ones(),
            mover_other: (mover & !good).count_ones(),
            opponent_good: (waiting & !good).count_ones(),
            opponent_other: (waiting & good).count_ones(),
        }
    }

    pub fn play(&mut self, col: u32) {
        debug_assert!(self.can_play(col));
        let spot = self.spot(col);
        self.current = self.current ^ self.mask;
        self.mask = self.mask | spot;
    }

    pub fn key(&self) -> G::Bits {
        self.current + self.mask
    }

    pub fn canonical(&self) -> G::Bits {
        let mirrored = Self::mirror(self.current) + Self::mirror(self.mask);
        self.key().min(mirrored)
    }

    #[must_use]
    pub fn from_moves(moves: &str) -> Option<Self> {
        moves.chars().try_fold(Self::default(), Self::push)
    }

    fn push(mut self, ch: char) -> Option<Self> {
        let col = ch.to_digit(10)?.checked_sub(1)?;
        if col >= G::COLUMNS || !self.can_play(col) || self.wins(col) {
            return None;
        }
        self.play(col);
        Some(self)
    }

    fn spot(&self, col: u32) -> G::Bits {
        (self.mask + Self::bottom(col)) & Self::column(col)
    }

    fn playable_cells(&self) -> G::Bits {
        (0..G::COLUMNS).fold(G::Bits::ZERO, |cells, col| cells | self.spot(col))
    }

    fn field() -> G::Bits {
        (0..G::COLUMNS).fold(G::Bits::ZERO, |all, col| all | Self::column(col))
    }

    fn first_rows() -> G::Bits {
        let stripe = (0..G::ROWS)
            .filter(|&row| Self::first_takes_row(row))
            .fold(G::Bits::ZERO, |cells, row| cells | (G::Bits::ONE << row));
        (0..G::COLUMNS).fold(G::Bits::ZERO, |all, col| {
            all | (stripe << (col * Self::STRIDE))
        })
    }

    const fn first_takes_row(row: u32) -> bool {
        ((G::COLUMNS - 1) * G::ROWS + row).is_multiple_of(2)
    }

    fn bottom(col: u32) -> G::Bits {
        G::Bits::ONE << (col * Self::STRIDE)
    }

    fn top(col: u32) -> G::Bits {
        Self::bottom(col) << (G::ROWS - 1)
    }

    fn column(col: u32) -> G::Bits {
        ((G::Bits::ONE << G::ROWS) - G::Bits::ONE) << (col * Self::STRIDE)
    }

    fn mirror(bits: G::Bits) -> G::Bits {
        (0..G::COLUMNS).fold(G::Bits::ZERO, |mirrored, col| {
            let cells = (bits >> (col * Self::STRIDE)) & Self::column(0);
            mirrored | (cells << ((G::COLUMNS - 1 - col) * Self::STRIDE))
        })
    }

    fn threats(stones: G::Bits) -> G::Bits {
        [1, Self::STRIDE - 1, Self::STRIDE, Self::STRIDE + 1]
            .into_iter()
            .fold(G::Bits::ZERO, |found, step| {
                let ahead = stones >> step;
                let behind = stones << step;
                let ahead_pair = ahead & (ahead >> step);
                let behind_pair = behind & (behind << step);
                found
                    | (ahead_pair & (ahead >> (2 * step)))
                    | (behind_pair & (behind << (2 * step)))
                    | (behind & ahead_pair)
                    | (behind_pair & ahead)
            })
    }

    fn winning_cells(stones: G::Bits, mask: G::Bits) -> G::Bits {
        Self::threats(stones) & Self::field() & !mask
    }
}

#[must_use]
pub fn format_moves(columns: &[u32]) -> String {
    columns.iter().map(|col| (col + 1).to_string()).collect()
}

const fn centre_first<G: Geometry>(step: u32) -> u32 {
    let centre = G::COLUMNS / 2;
    let distance = step.div_ceil(2);
    if step.is_multiple_of(2) {
        centre + distance
    } else {
        centre - distance
    }
}
