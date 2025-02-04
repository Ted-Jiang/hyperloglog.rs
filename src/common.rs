use serde::{Deserialize, Serialize};

#[cfg(feature = "alloc")]
use alloc::{vec, vec::Vec};

// A macro to create `Registers` structs for different Register sizes.
macro_rules! registers_impls {
    ($len:expr, $ident:ident) => {
        // A Registers struct.
        #[derive(Clone, Debug, Serialize, Deserialize)]
        pub struct $ident {
            // A buffer containing registers.
            buf:   Vec<u32>,
            // The number of registers stored in buf.
            count: usize,
            // The number of registers set to zero.
            zeros: usize,
        }

        impl $ident {
            // The register's size (in bits).
            pub const SIZE: usize = $len;
            // The number of registers that fit in a 32-bit integer.
            const COUNT_PER_WORD: usize = 32 / Self::SIZE;
            // A mask to get the lower register (from LSB).
            const MASK: u32 = (1 << Self::SIZE) - 1;

            // Creates a new Registers struct with capacity `count` registers.
            pub fn with_count(count: usize) -> $ident {
                $ident {
                    buf:   vec![0; ceil(count, Self::COUNT_PER_WORD)],
                    count: count,
                    zeros: count,
                }
            }

            #[inline] // Returns an iterator that emits Register values.
            pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
                self.buf
                    .iter()
                    .map(|val| {
                        (0..Self::COUNT_PER_WORD).map(move |i| {
                            ((val >> i * Self::SIZE) & Self::MASK)
                        })
                    })
                    .flatten()
                    .take(self.count)
            }

            #[inline] // Returns the value of the Register at `index`.
            #[allow(dead_code)]
            pub fn get(&self, index: usize) -> u32 {
                let (qu, rm) = (
                    index / Self::COUNT_PER_WORD,
                    index % Self::COUNT_PER_WORD,
                );

                (self.buf[qu] >> (rm * Self::SIZE)) & Self::MASK
            }

            #[inline] // Sets the value of the Register at `index` to `value`,
                      // if `value` is greater than its current value.
            pub fn set_greater(&mut self, index: usize, value: u32) {
                let (qu, rm) = (
                    index / Self::COUNT_PER_WORD,
                    index % Self::COUNT_PER_WORD,
                );

                let cur = (self.buf[qu] >> (rm * Self::SIZE)) & Self::MASK;

                if value > cur {
                    if cur == 0 {
                        self.zeros -= 1;
                        self.buf[qu] |= (value << (rm * Self::SIZE));
                    } else {
                        let mask = Self::MASK << (rm * Self::SIZE);

                        self.buf[qu] = (self.buf[qu] & !mask) |
                            (value << (rm * Self::SIZE));
                    }
                }
            }

            #[inline]
            pub fn zeros(&self) -> usize {
                self.zeros
            }

            #[inline] // Returns the size of the Registers in bytes
            #[allow(dead_code)] // for a given number of Registers.
            pub fn size_in_bytes(count: usize) -> usize {
                4 * count / Self::COUNT_PER_WORD
            }
        }
    };
}

// Registers implementation for 5-bit registers,
// used by HyperLogLog original implementation.
//
// Available also with no_std.
registers_impls![5, Registers];

// Registers implementation for 6-bit registers,
// used by HyperLogLog++ implementation.
//
// Available only with std.
//Align with Kylin: use 8 bit to store one hash
#[cfg(feature = "std")]
//registers_impls![8, RegistersPlus];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegistersPlus {
    // A buffer containing registers.
    buf: Vec<u8>,
    // The number of registers stored in buf.
    count: usize,
    // The number of registers set to zero.
    zeros: usize,
}

impl RegistersPlus {

    #[allow(dead_code)]
    pub const SIZE: usize = 8;

    // Creates a new Registers struct with capacity `count` registers.
    pub fn with_count(count: usize) -> RegistersPlus {
        RegistersPlus {
            buf: vec![0; count],
            count: count,
            zeros: count,
        }
    }

    #[inline] // Returns an iterator that emits Register values.
    pub fn iter(&self) -> impl Iterator<Item=u8> + '_ {
        self.buf.iter().map(|x| *x)
    }

    #[inline] // Returns the value of the Register at `index`.
    #[allow(dead_code)]
    pub fn get(&self, index: usize) -> u8 {
        self.buf[index]
    }

    #[inline] // Sets the value of the Register at `index` to `value`,
    // if `value` is greater than its current value.
    pub fn set_greater(&mut self, index: usize, value: u8) {
        let cur = self.buf[index];
        if value > cur {
            if cur == 0 {
                self.zeros -= 1;
                self.buf[index] = value;
            } else {
                self.buf[index] = value;
            }
        }
    }

    #[inline]
    pub fn zeros(&self) -> usize {
        self.zeros
    }

    #[inline] // Returns the size of the Registers in bytes
    #[allow(dead_code)] // for a given number of Registers.
    pub fn size_in_bytes(count: usize) -> usize {
        count
    }
}

// An array containing all possible values used to calculate
// the "raw" sum.
//
// Instead of computing those values every time, look them up here.
//
// This is used only in the case the `const-loop` feature is enabled,
// it requires a Rust compiler version 1.45.0 or higher.
macro_rules! rawlut_impls {
    ($ident:ident) => {
        #[cfg(feature = "const-loop")]
        const RAW: [f64; 1 << $ident::SIZE] = {
            const COUNT: usize = 1 << $ident::SIZE;

            let mut raw = [0.0; COUNT];

            let mut i = 0;
            while i < COUNT && i < 128 {
                raw[i] = 1.0 / (1u128 << i) as f64;
                i += 1;
            }

            while i < COUNT {
                raw[i] = raw[i-1] / 2.0 ;
                i += 1;
            }

            raw
        };
    };
}

#[cfg(not(feature = "std"))]
rawlut_impls![Registers];

#[cfg(feature = "std")]
rawlut_impls![RegistersPlus];

// A trait for sharing common HyperLogLog related functionality between
// different HyperLogLog implementations.
pub trait HyperLogLogCommon {
    #[inline] // Returns the "raw" HyperLogLog estimate as defined by
    // P. Flajolet et al. for a given `precision`.
    //
    // Also returns the count of registers set to 0.
    fn estimate_raw<I>(registers: I, count: usize) -> (f64, usize)
        where
            I: Iterator<Item=u32>,
    {
        let (mut raw, mut zeros) = (0.0, 0);

        for value in registers {
            raw += 1.0 / (1u64 << value) as f64;
            zeros += if value == 0 { 1 } else { 0 };
        }

        raw = Self::alpha(count) * (count * count) as f64 / raw;

        (raw, zeros)
    }

    #[cfg(not(feature = "const-loop"))]
    #[inline] // Returns the "raw" HyperLogLog estimate as defined by
    // P. Flajolet et al. for a given `precision`.
    fn estimate_raw_plus<I>(registers: I, count: usize) -> f64
        where
            I: Iterator<Item=u32>,
    {
        let raw: f64 = registers.map(|val| 1.0 / (1u64 << val) as f64).sum();

        Self::alpha(count) * (count * count) as f64 / raw
    }

    #[cfg(feature = "const-loop")]
    #[inline] // Returns the "raw" HyperLogLog estimate as defined by
    // P. Flajolet et al. for a given `precision`.
    fn estimate_raw_plus<I>(registers: I, count: usize) -> f64
        where
            I: Iterator<Item=u32>,
    {
        let raw: f64 = registers.map(|val| RAW[val as usize]).sum();

        Self::alpha(count) * (count * count) as f64 / raw
    }

    #[cfg(not(feature = "const-loop"))]
    #[inline] // Returns the "raw" HyperLogLog estimate as defined by
    // P. Flajolet et al. for a given `precision`.
    fn estimate_raw_plus_u8<I>(registers: I, count: usize) -> f64
        where
            I: Iterator<Item=u8>,
    {
        let raw: f64 = registers.map(|val| 1.0 / (1u64 << val) as f64).sum();

        Self::alpha(count) * (count * count) as f64 / raw
    }

    #[cfg(feature = "const-loop")]
    #[inline] // Returns the "raw" HyperLogLog estimate as defined by
    // P. Flajolet et al. for a given `precision`.
    fn estimate_raw_plus_u8<I>(registers: I, count: usize) -> f64
        where
            I: Iterator<Item=u8>,
    {
        let raw: f64 = registers.map(|val| RAW[val as usize]).sum();

        Self::alpha(count) * (count * count) as f64 / raw
    }

    #[inline] // Estimates the count of distinct elements using linear
    // counting.
    fn linear_count(count: usize, zeros: usize) -> f64 {
        count as f64 * ln(count as f64 / zeros as f64)
    }

    #[inline] // Returns the alpha constant based on precision.
    fn alpha(count: usize) -> f64 {
        match count {
            16 => 0.673,
            32 => 0.697,
            64 => 0.709,
            _ => 0.7213 / (1.0 + 1.079 / count as f64),
        }
    }

    #[inline] // Returns the number of registers based on precision.
    fn register_count(precision: u8) -> usize {
        1 << precision
    }
}

#[inline] // Returns the int ceil of num, denom.
pub fn ceil(num: usize, denom: usize) -> usize {
    (num + denom - 1) / denom
}

#[cfg(feature = "std")]
#[inline]
pub fn ln(x: f64) -> f64 {
    x.ln()
}

#[cfg(not(feature = "std"))]
#[inline]
pub fn ln(x: f64) -> f64 {
    crate::log::log(x)
}

// A trait for extracting a range of bits from a value.
pub trait BitExtract<T> {
    // Extracts bits nums(hi..lo], with hi exclusive using LSB 0 indexing.
    fn extract(num: T, hi: u8, lo: u8) -> T;
}

impl BitExtract<u64> for u64 {
    #[inline]
    fn extract(num: u64, hi: u8, lo: u8) -> u64 {
        (num << (64 - hi)) >> (64 - (hi - lo))
    }
}

impl BitExtract<u32> for u32 {
    #[inline]
    fn extract(num: u32, hi: u8, lo: u8) -> u32 {
        (num << (32 - hi)) >> (32 - (hi - lo))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registers_get_set() {
        let mut registers: Registers = Registers::with_count(10);

        assert_eq!(registers.buf.len(), 2);

        registers.set_greater(1, 0b11);

        assert_eq!(registers.buf, vec![0b1100000, 0]);

        registers.set_greater(9, 0x7);

        assert_eq!(registers.buf, vec![0b1100000, 0x038000]);
    }

    #[cfg(feature = "std")]
    #[test]
    #[ignore]
    fn test_registers_get_set_plus() {
        let mut registers: RegistersPlus = RegistersPlus::with_count(10);

        assert_eq!(registers.buf.len(), 2);

        registers.set_greater(1, 0b11);

        assert_eq!(registers.buf, vec![0b11000000, 0]);

        registers.set_greater(9, 0x7);

        assert_eq!(registers.buf, vec![0b11000000, 0b01000000]);
    }

    #[test]
    fn test_registers_set_greater() {
        let mut registers: Registers = Registers::with_count(10);

        assert_eq!(registers.buf.len(), 2);

        assert_eq!(registers.zeros(), 10);

        registers.set_greater(1, 0);

        assert_eq!(registers.buf, vec![0, 0]);
        assert_eq!(registers.zeros(), 10);

        registers.set_greater(1, 0b11);

        assert_eq!(registers.buf, vec![0b1100000, 0]);
        assert_eq!(registers.zeros(), 9);

        registers.set_greater(9, 0x7);

        assert_eq!(registers.buf, vec![0b1100000, 0x038000]);
        assert_eq!(registers.zeros(), 8);

        registers.set_greater(1, 0b10);

        assert_eq!(registers.buf, vec![0b1100000, 0x038000]);
        assert_eq!(registers.zeros(), 8);

        registers.set_greater(9, 0x9);

        assert_eq!(registers.buf, vec![0b1100000, 0x048000]);
        assert_eq!(registers.zeros(), 8);
    }

    #[cfg(feature = "std")]
    #[test]
    #[ignore]
    fn test_registers_set_greater_plus() {
        let mut registers: RegistersPlus = RegistersPlus::with_count(10);

        assert_eq!(registers.buf.len(), 2);

        assert_eq!(registers.zeros(), 10);

        registers.set_greater(1, 0);

        assert_eq!(registers.buf, vec![0, 0]);
        assert_eq!(registers.zeros(), 10);

        registers.set_greater(1, 0b11);

        assert_eq!(registers.buf, vec![0b11000000, 0]);
        assert_eq!(registers.zeros(), 9);

        registers.set_greater(9, 0x7);

        assert_eq!(registers.buf, vec![0b11000000, 0b01000000]);
        assert_eq!(registers.zeros(), 8);

        registers.set_greater(1, 0b10);

        assert_eq!(registers.buf, vec![0b11000000, 0b01000000]);
        assert_eq!(registers.zeros(), 8);

        registers.set_greater(9, 0x9);

        assert_eq!(registers.buf, vec![0b11000000, 0b01010000]);
        assert_eq!(registers.zeros(), 8);
    }

    #[test]
    fn test_extract() {
        let num = 0b0010101110101101;

        assert_eq!(u64::extract(num, 7, 3), 0b0101);
        assert_eq!(u32::extract(num as u32, 7, 3), 0b0101);

        assert_eq!(u64::extract(num, 8, 3), 0b10101);
        assert_eq!(u32::extract(num as u32, 8, 3), 0b10101);

        assert_eq!(u64::extract(num, 15, 14), 0b0);
        assert_eq!(u32::extract(num as u32, 15, 14), 0b0);

        assert_eq!(u64::extract(num, 15, 13), 0b01);
        assert_eq!(u32::extract(num as u32, 15, 13), 0b01);
    }

    #[cfg(feature = "bench-units")]
    mod benches {
        extern crate test;

        use super::*;
        use test::{black_box, Bencher};

        #[bench]
        fn bench_registers_get(b: &mut Bencher) {
            let registers: RegistersPlus = RegistersPlus::with_count(1000);

            b.iter(|| {
                for i in 0..1000 {
                    let val = registers.get(i);
                    black_box(val);
                }
            })
        }

        #[bench]
        fn bench_registers_set(b: &mut Bencher) {
            let mut registers: RegistersPlus = RegistersPlus::with_count(1000);

            b.iter(|| {
                for i in 0..1000 {
                    registers.set_greater(i, 123);
                }
            })
        }
    }
}
