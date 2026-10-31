use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Add, BitAnd, BitOr, BitXor, Not, Shl, Shr, Sub};

pub trait Bits:
    Copy
    + Send
    + Sync
    + Eq
    + Ord
    + Hash
    + Debug
    + Add<Output = Self>
    + Sub<Output = Self>
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + Not<Output = Self>
    + Shl<u32, Output = Self>
    + Shr<u32, Output = Self>
{
    const ZERO: Self;
    const ONE: Self;
    const BITS: u32;
    fn mix(self) -> usize;
    fn count_ones(self) -> u32;
}

const FNV1A32_OFFSET_BASIS: usize = 0x811c_9dc5;
const FNV1A32_PRIME: usize = 0x0100_0193;

macro_rules! bits {
    ($($t:ty),*) => {
        $(
            impl Bits for $t {
                const ZERO: Self = 0;
                const ONE: Self = 1;
                const BITS: u32 = <$t>::BITS;
                fn mix(self) -> usize {
                    self.to_le_bytes().into_iter().fold(FNV1A32_OFFSET_BASIS, |hash, byte| {
                        (hash ^ usize::from(byte)).wrapping_mul(FNV1A32_PRIME)
                    })
                }
                fn count_ones(self) -> u32 {
                    <$t>::count_ones(self)
                }
            }
        )*
    };
}

bits!(u64, u128);
