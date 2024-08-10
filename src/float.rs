use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, ShlAssign, ShrAssign, Sub, SubAssign};

const WORD_WIDTH: usize = 32;

/// Wide float specialized for use in Mandelbrot calculations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WideFloat(Vec<u32>);

fn isolate_mantissa(f: f32) -> u32 {
    f.to_bits() & 0x7f_ffff
}

fn isolate_exponent(f: f32) -> u32 {
    (f.to_bits() >> (f32::MANTISSA_DIGITS - 1)) & 0xff
}

#[derive(Clone, Copy, Debug)]
pub enum FromFloatError {
    IsNan,
    OutOfRange,
}

impl WideFloat {
    pub fn zero(size: usize) -> Self {
        Self(vec![0; size])
    }

    pub fn min_positive(size: usize) -> Self {
        let mut buffer = vec![0; size];
        buffer[0] = 1;
        Self(buffer)
    }

    pub fn from_i32(value: i32, size: usize) -> Self {
        let mut buffer = vec![0; size];
        buffer[size - 1] = u32::from_ne_bytes(i32::to_ne_bytes(value));
        Self(buffer)
    }

    pub fn from_f32(value: f32, size: usize) -> Result<Self, FromFloatError> {
        let (neg, value) = if value < 0.0 {
            (true, -value)
        } else {
            (false, value)
        };
        let e = isolate_exponent(value);
        // Note: This rounds subnormal numbers to zero
        if e == 0 {
            return Ok(WideFloat::zero(size));
        }
        let v = isolate_mantissa(value) << (WORD_WIDTH as u32 - f32::MANTISSA_DIGITS)
            | 1 << (WORD_WIDTH - 1);

        let shift = 0x7e_i32 - e as i32 + WORD_WIDTH as i32;
        let offset = shift as usize / WORD_WIDTH;

        let left = v >> (shift % WORD_WIDTH as i32);
        let right = if shift % WORD_WIDTH as i32 != 0 {
            v << (WORD_WIDTH - shift as usize % WORD_WIDTH)
        } else {
            0
        };

        let mut buffer = vec![0; size];

        if let Some(v) = buffer.get_mut(offset) {
            *v = left;
        }
        if let Some(v) = buffer.get_mut(offset + 1) {
            *v = right;
        }

        // TODO: return error on unsupported values

        buffer.reverse();
        if neg {
            Ok(-Self(buffer))
        } else {
            Ok(Self(buffer))
        }
    }

    pub fn from_raw(buffer: Vec<u32>) -> Self {
        Self(buffer)
    }

    // TODO: bring back f64 conversions
    pub fn as_f32_round(&self) -> f32 {
        if self.0.iter().all(|w| *w == 0) {
            return 0.0;
        }
        let neg = self < &0;
        let mut zero_words = 0;
        let mut first_word = 0;
        let mut second_word = 0;
        let mut carry = true;
        for mut word in self.0.iter().copied() {
            if neg {
                (word, carry) = (!word).overflowing_add(carry as u32);
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
        let mantissa = (1u32 << (WORD_WIDTH - 1 - word_zeros as usize)) ^ first_word;
        let exponent = 0x7e_u32 - (word_zeros + WORD_WIDTH as u32 * zero_words) + WORD_WIDTH as u32;

        let shift = word_zeros as i32 - WORD_WIDTH as i32 + f32::MANTISSA_DIGITS as i32;
        let v = if shift <= 0 {
            mantissa >> -shift
        } else {
            mantissa << shift | second_word >> (WORD_WIDTH - shift as usize)
        };

        let f = f32::from_bits((exponent << (f32::MANTISSA_DIGITS - 1)) | v);
        if neg {
            -f
        } else {
            f
        }
    }

    pub fn floor(&self) -> i32 {
        i32::from_ne_bytes(self.0.last().unwrap().to_ne_bytes())
    }

    pub fn is_int(&self) -> bool {
        self.0.iter().take(self.0.len() - 1).all(|p| *p == 0)
    }

    pub fn word_count(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the least significant word of a positive number is less than or equal to `threshold` and the
    /// remaining words are zero
    pub fn requires_precision(&self, threshold: u32) -> bool {
        self.0[0] <= threshold && self.0.iter().skip(1).all(|p| *p == 0)
    }

    /// Returns true if the second least significant word of a positive number is greater than or equal to `threshold`
    pub fn excess_precision(&self, threshold: u32) -> bool {
        self.0[1] >= threshold
    }

    /// Increases the word count of this number by adding a zeroed least significant word
    pub fn increase_precision(&mut self) {
        self.0.insert(0, 0);
    }

    /// Decreases the word count of this number by removing the least significant word
    pub fn decrease_precision(&mut self) {
        self.0.remove(0);
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.0)
    }
}

impl PartialEq<i32> for WideFloat {
    fn eq(&self, other: &i32) -> bool {
        self.floor() == *other && self.is_int()
    }
}

impl PartialOrd<i32> for WideFloat {
    fn partial_cmp(&self, other: &i32) -> Option<std::cmp::Ordering> {
        let ord = self.floor().cmp(other);
        if ord.is_eq() && !self.is_int() {
            Some(std::cmp::Ordering::Greater)
        } else {
            Some(ord)
        }
    }
}

impl Add<&Self> for WideFloat {
    type Output = WideFloat;

    fn add(mut self, rhs: &Self) -> Self::Output {
        assert_eq!(self.0.len(), rhs.0.len());

        let mut carry = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(&rhs.0) {
            (*lhs_part, carry) = lhs_part.carrying_add(*rhs_part, carry);
        }
        self
    }
}

impl AddAssign<&Self> for WideFloat {
    fn add_assign(&mut self, rhs: &Self) {
        assert_eq!(self.0.len(), rhs.0.len());

        let mut carry = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(&rhs.0) {
            (*lhs_part, carry) = lhs_part.carrying_add(*rhs_part, carry);
        }
    }
}

impl Sub<&Self> for WideFloat {
    type Output = Self;

    fn sub(mut self, rhs: &Self) -> Self {
        assert_eq!(self.0.len(), rhs.0.len());

        let mut borrow = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(&rhs.0) {
            (*lhs_part, borrow) = lhs_part.borrowing_sub(*rhs_part, borrow);
        }
        self
    }
}

impl SubAssign<&Self> for WideFloat {
    fn sub_assign(&mut self, rhs: &Self) {
        assert_eq!(self.0.len(), rhs.0.len());

        let mut borrow = false;
        for (lhs_part, rhs_part) in self.0.iter_mut().zip(&rhs.0) {
            (*lhs_part, borrow) = lhs_part.borrowing_sub(*rhs_part, borrow);
        }
    }
}

impl Mul for &WideFloat {
    type Output = WideFloat;

    fn mul(self, rhs: Self) -> Self::Output {
        let len = self.0.len();
        assert_eq!(len, rhs.0.len());

        let lneg = self.floor() < 0;
        let rneg = rhs.floor() < 0;
        let mut result = WideFloat::zero(len);
        let mut carry = true;
        for (l_idx, l_word) in self
            .0
            .iter()
            .copied()
            .map(|w| {
                if lneg {
                    let neg_w;
                    (neg_w, carry) = (!w).overflowing_add(carry as u32);
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
            let shift = len - l_idx - 1;
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

impl MulAssign<&WideFloat> for WideFloat {
    fn mul_assign(&mut self, rhs: &Self) {
        *self = &*self * rhs;
    }
}

impl ShrAssign<usize> for WideFloat {
    fn shr_assign(&mut self, rhs: usize) {
        let len = self.0.len();

        let rotate = rhs / WORD_WIDTH;
        self.0.copy_within(rotate.., 0);
        self.0.iter_mut().skip(len - rotate).for_each(|w| *w = 0);

        let shift = rhs % WORD_WIDTH;
        if shift != 0 {
            let mut carry = 0;
            for w in self.0.iter_mut().rev().take(len - rotate) {
                let tmp = (*w >> shift) + carry;
                carry = *w << (WORD_WIDTH - shift);
                *w = tmp;
            }
        }
    }
}

impl ShlAssign<usize> for WideFloat {
    fn shl_assign(&mut self, rhs: usize) {
        let len = self.0.len();

        let rotate = rhs / WORD_WIDTH;
        self.0.copy_within(..len - rotate, rotate);
        self.0.iter_mut().take(rotate).for_each(|w| *w = 0);

        let shift = rhs % WORD_WIDTH;
        if shift != 0 {
            let mut carry = 0;
            for w in self.0.iter_mut().take(len - rotate) {
                let tmp = (*w << shift) + carry;
                carry = *w >> (WORD_WIDTH - shift);
                *w = tmp;
            }
        }
    }
}

impl Neg for WideFloat {
    type Output = Self;

    fn neg(mut self) -> Self::Output {
        let mut carry = true;
        self.0
            .iter_mut()
            .for_each(|w| (*w, carry) = (!*w).overflowing_add(carry as u32));
        self
    }
}

impl PartialOrd for WideFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WideFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        assert_eq!(self.0.len(), other.0.len());

        let whole_cmp = self.floor().cmp(&other.floor());
        if whole_cmp != std::cmp::Ordering::Equal {
            return whole_cmp;
        }
        self.0
            .iter()
            .rev()
            .skip(1)
            .cmp(other.0.iter().rev().skip(1))
    }
}
