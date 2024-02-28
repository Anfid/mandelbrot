use std::{
    convert::TryFrom,
    ops::{Add, AddAssign, Mul, MulAssign, Neg, ShlAssign, ShrAssign, Sub, SubAssign},
};

const WORD_WIDTH: usize = 64;

/// Wide float specialized for use in Mandelbrot calculations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WideFloat<const N: usize>([u64; N]);

impl<const N: usize> Default for WideFloat<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

fn isolate_mantissa(f: f64) -> u64 {
    f.to_bits() & 0xf_ffff_ffff_ffff
}

fn isolate_exponent(f: f64) -> u32 {
    (f.to_bits() >> 52) as u32 & 0x7ff
}

#[derive(Clone, Copy, Debug)]
pub enum FromFloatError {
    IsNan,
    OutOfRange,
}

impl<const N: usize> TryFrom<f64> for WideFloat<N> {
    type Error = FromFloatError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        let (neg, value) = if value < 0.0 {
            (true, -value)
        } else {
            (false, value)
        };
        let e = isolate_exponent(value);
        // Note: This rounds subnormal numbers to zero
        if e == 0 {
            return Ok(WideFloat::default());
        }
        let v = isolate_mantissa(value) << WORD_WIDTH as u32 - f64::MANTISSA_DIGITS
            | 1 << WORD_WIDTH - 1;

        let shift = 0x3fe_i64 - e as i64 + 64;
        let offset = shift as usize / 64;

        let left = v >> shift % 64;
        let right = if shift % 64 != 0 {
            v << 64 - shift % 64
        } else {
            0
        };

        let mut buffer = [0; N];

        buffer.get_mut(offset).map(|v| *v = left);
        buffer.get_mut(offset + 1).map(|v| *v = right);

        // TODO: return error on unsupported values
        // TODO: support negative values

        buffer.reverse();
        if neg {
            Ok(-Self(buffer))
        } else {
            Ok(Self(buffer))
        }
    }
}

impl<const N: usize> From<i64> for WideFloat<N> {
    fn from(value: i64) -> Self {
        let mut buffer = [0; N];
        buffer[N - 1] = u64::from_ne_bytes(i64::to_ne_bytes(value));
        Self(buffer)
    }
}

impl<const N: usize> WideFloat<N> {
    pub fn as_f64_round(&self) -> f64 {
        if self.0.into_iter().all(|w| w == 0) {
            return 0.0;
        }
        let neg = self < &0;
        let mut zero_words = 0;
        let mut first_word = 0;
        let mut second_word = 0;
        let mut carry = true;
        for mut word in self.0.into_iter() {
            if neg {
                (word, carry) = (!word).overflowing_add(carry as u64);
            }
            if word != 0 {
                second_word = first_word;
                first_word = word;
                zero_words = 0;
            } else {
                zero_words += 1;
            }
        }
        let word_zeros = first_word.leading_zeros();
        let mantissa = (1u64 << (63 - word_zeros)) ^ first_word;
        let exponent =
            0x3fe_u64 - (word_zeros as u64 + WORD_WIDTH as u64 * zero_words) + WORD_WIDTH as u64;

        let shift = word_zeros as i32 - WORD_WIDTH as i32 + f64::MANTISSA_DIGITS as i32;
        let v = if shift <= 0 {
            mantissa >> -shift
        } else {
            mantissa << shift | second_word >> (WORD_WIDTH - shift as usize)
        };

        let f = f64::from_bits((exponent << 52) | v);
        if neg {
            -f
        } else {
            f
        }
    }

    pub fn floor(&self) -> i64 {
        i64::from_ne_bytes(self.0[N - 1].to_ne_bytes())
    }

    pub fn is_int(&self) -> bool {
        self.0.into_iter().take(N - 1).all(|p| p == 0)
    }
}

impl<const N: usize> PartialEq<i64> for WideFloat<N> {
    fn eq(&self, other: &i64) -> bool {
        self.floor() == *other && self.is_int()
    }
}

impl<const N: usize> PartialOrd<i64> for WideFloat<N> {
    fn partial_cmp(&self, other: &i64) -> Option<std::cmp::Ordering> {
        let ord = self.floor().cmp(other);
        if ord.is_eq() && !self.is_int() {
            Some(std::cmp::Ordering::Greater)
        } else {
            Some(ord)
        }
    }
}

impl<const N: usize> Add<&Self> for WideFloat<N> {
    type Output = WideFloat<N>;

    fn add(mut self, rhs: &Self) -> Self::Output {
        let mut carry = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(rhs.0) {
            (*lhs_part, carry) = lhs_part.carrying_add(rhs_part, carry);
        }
        self
    }
}

impl<const N: usize> AddAssign<&Self> for WideFloat<N> {
    fn add_assign(&mut self, rhs: &Self) {
        let mut carry = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(rhs.0) {
            (*lhs_part, carry) = lhs_part.carrying_add(rhs_part, carry);
        }
    }
}

impl<const N: usize> Sub<&Self> for WideFloat<N> {
    type Output = Self;

    fn sub(mut self, rhs: &Self) -> Self {
        let mut borrow = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(rhs.0) {
            (*lhs_part, borrow) = lhs_part.borrowing_sub(rhs_part, borrow);
        }
        self
    }
}

impl<const N: usize> SubAssign<&Self> for WideFloat<N> {
    fn sub_assign(&mut self, rhs: &Self) {
        let mut borrow = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(rhs.0) {
            (*lhs_part, borrow) = lhs_part.borrowing_sub(rhs_part, borrow);
        }
    }
}

impl<const N: usize> Mul for &WideFloat<N> {
    type Output = WideFloat<N>;

    fn mul(self, rhs: Self) -> Self::Output {
        let lneg = self.floor() < 0;
        let rneg = rhs.floor() < 0;
        let mut result = WideFloat::default();
        let mut carry = true;
        for (l_idx, l_word) in self
            .0
            .into_iter()
            .map(|w| {
                if lneg {
                    let neg_w;
                    (neg_w, carry) = (!w).overflowing_add(carry as u64);
                    neg_w
                } else {
                    w
                }
            })
            .enumerate()
        {
            let mut part = rhs.clone();
            if rneg {
                part = -part
            }
            let mut carry = 0;
            for r_word in part.0.iter_mut() {
                (*r_word, carry) = l_word.carrying_mul(*r_word, carry);
            }
            let shift = N - l_idx - 1;
            part >>= shift * WORD_WIDTH;
            if carry != 0 {
                part.0[l_idx + 1] = carry;
            }
            result += &part;
        }
        if rneg ^ lneg {
            result = -result;
        }
        result
    }
}

impl<const N: usize> MulAssign<&WideFloat<N>> for WideFloat<N> {
    fn mul_assign(&mut self, rhs: &Self) {
        *self = &*self * rhs;
    }
}

impl<const N: usize> ShrAssign<usize> for WideFloat<N> {
    fn shr_assign(&mut self, rhs: usize) {
        let rotate = rhs / WORD_WIDTH;
        self.0.copy_within(rotate.., 0);
        self.0.iter_mut().skip(N - rotate).for_each(|w| *w = 0);

        let shift = rhs % WORD_WIDTH;
        if shift != 0 {
            let mut carry = 0;
            for w in self.0.iter_mut().skip(rotate) {
                let tmp = (*w >> shift) + carry;
                carry = *w << WORD_WIDTH - shift;
                *w = tmp;
            }
        }
    }
}

impl<const N: usize> ShlAssign<usize> for WideFloat<N> {
    fn shl_assign(&mut self, rhs: usize) {
        let rotate = rhs / WORD_WIDTH;
        self.0.copy_within(..N - rotate, rotate);
        self.0.iter_mut().take(rotate).for_each(|w| *w = 0);

        let shift = rhs % WORD_WIDTH;
        if shift != 0 {
            let mut carry = 0;
            for w in self.0.iter_mut().take(N - rotate) {
                let tmp = (*w << shift) + carry;
                carry = *w >> WORD_WIDTH - shift;
                *w = tmp;
            }
        }
    }
}

impl<const N: usize> Neg for WideFloat<N> {
    type Output = Self;

    fn neg(mut self) -> Self::Output {
        let mut carry = true;
        self.0
            .iter_mut()
            .for_each(|w| (*w, carry) = (!*w).overflowing_add(carry as u64));
        self
    }
}
