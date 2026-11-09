//! Writes std/koji/ryu.kj, the tables of powers of 5 with which the library written in Koji finds
//! the shortest decimal of a float (Ulf Adams' Ryu, in std/koji/decimal.kj):
//!
//!     rustc -O bootstrap/ryu.rs -o build/ryu && build/ryu > std/koji/ryu.kj
//!
//! `powersOfFive[i]` is 5^i in its first 125 bits, for i below 326; `inversePowersOfFive[i]` is
//! 2^(bits of 5^i - 1 + 125) / 5^i + 1, for i below 342. Each is a record of 32 hexadecimal
//! digits: its high 64 bits, then its low 64.

/// A natural number, in words of 32 bits from the lowest.
#[derive(Clone)]
struct Big(Vec<u32>);

impl Big {
    fn one() -> Big {
        Big(vec![1])
    }

    fn times(&self, factor: u32) -> Big {
        let mut words = Vec::with_capacity(self.0.len() + 1);
        let mut carry = 0u64;
        for &word in &self.0 {
            let product = word as u64 * factor as u64 + carry;
            words.push(product as u32);
            carry = product >> 32;
        }
        if carry > 0 {
            words.push(carry as u32);
        }
        Big(words)
    }

    fn bits(&self) -> usize {
        let top = self.0.len() - 1;
        top * 32 + (32 - self.0[top].leading_zeros() as usize)
    }

    fn bit(&self, index: usize) -> bool {
        self.0.get(index / 32).map_or(false, |word| word >> (index % 32) & 1 == 1)
    }

    /// The number shifted right by `n` bits (n at least 0), as at most 128 bits.
    fn shifted_right(&self, n: usize) -> u128 {
        (0..128).fold(0u128, |value, i| if self.bit(n + i) { value | 1 << i } else { value })
    }

    fn at_least(&self, other: &Big) -> bool {
        let length = self.0.len().max(other.0.len());
        for i in (0..length).rev() {
            let (a, b) = (self.0.get(i).copied().unwrap_or(0), other.0.get(i).copied().unwrap_or(0));
            if a != b {
                return a > b;
            }
        }
        true
    }

    fn minus(&self, other: &Big) -> Big {
        let mut words = Vec::with_capacity(self.0.len());
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let mut difference = self.0[i] as i64 - other.0.get(i).copied().unwrap_or(0) as i64 - borrow;
            borrow = 0;
            if difference < 0 {
                difference += 1 << 32;
                borrow = 1;
            }
            words.push(difference as u32);
        }
        while words.len() > 1 && *words.last().unwrap() == 0 {
            words.pop();
        }
        Big(words)
    }

    /// 2^power / self, rounded down: a division bit by bit (the quotient fits in 128 bits).
    fn divide_power_of_two(&self, power: usize) -> u128 {
        let mut remainder = Big(vec![0]);
        let mut quotient = 0u128;
        for bit in (0..=power).rev() {
            remainder = remainder.times(2);
            if bit == power {
                remainder.0[0] |= 1;
            }
            quotient <<= 1;
            if remainder.at_least(self) {
                remainder = remainder.minus(self);
                quotient |= 1;
            }
        }
        quotient
    }
}

fn record(value: u128) -> String {
    format!("{:016x}{:016x}", (value >> 64) as u64, value as u64)
}

fn main() {
    let mut powers = String::new();
    let mut inverses = String::new();
    let mut five = Big::one();
    for i in 0..342 {
        let bits = five.bits();
        if i < 326 {
            let value = if bits >= 125 { five.shifted_right(bits - 125) } else { five.shifted_right(0) << (125 - bits) };
            powers += &record(value);
        }
        inverses += &record(five.divide_power_of_two(bits - 1 + 125) + 1);
        five = five.times(5);
    }
    println!("// The tables of powers of 5 of Ryu (std/koji/decimal.kj): written by bootstrap/ryu.rs (do not");
    println!("// edit). Records of 32 hexadecimal digits, the high 64 bits of a number of 128 bits first.");
    println!();
    println!("// 5^i in its first 125 bits, for i below 326.");
    println!("export immut powersOfFive = '{}'", powers);
    println!();
    println!("// 2^(bits of 5^i - 1 + 125) / 5^i + 1, for i below 342.");
    println!("export immut inversePowersOfFive = '{}'", inverses);
}
