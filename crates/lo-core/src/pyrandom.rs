//! Python's `random` module (the Mersenne Twister and the methods OpenOrbit uses), giving the same numbers as CPython for
//! the same seed. The game seeds it from the clock, so the sequences only matter for comparisons: with the same seed and
//! the same calls in the same order, a ported function draws exactly what OpenOrbit draws.

const N: usize = 624;
const M: usize = 397;

/// A random generator equal to `random.Random`.
#[derive(Clone)]
pub struct Random {
    mt: [u32; N],
    index: usize,
}

impl std::fmt::Debug for Random {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Random {{ index: {} }}", self.index)
    }
}

impl Default for Random {
    /// Seeded from the clock (like `random.seed()` without urandom).
    fn default() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        Random::from_seed_u128(t)
    }
}

impl Random {
    fn init_genrand(s: u32) -> [u32; N] {
        let mut mt = [0u32; N];
        mt[0] = s;
        for i in 1..N {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        mt
    }

    fn init_by_array(key: &[u32]) -> Random {
        let mut mt = Self::init_genrand(19650218);
        let (mut i, mut j) = (1usize, 0usize);
        let mut k = N.max(key.len());
        while k > 0 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
            k -= 1;
        }
        k = N - 1;
        while k > 0 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1566083941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
            k -= 1;
        }
        mt[0] = 0x8000_0000;
        Random { mt, index: N }
    }

    /// `random.Random(n)` for a non-negative integer (Python uses the absolute value).
    pub fn new(seed: i64) -> Random {
        Self::from_seed_u128(u128::from(seed.unsigned_abs()))
    }

    /// `random.Random(s)` for a string (seed version 2: the UTF-8 bytes followed by their SHA-512, read as a big-endian
    /// integer).
    pub fn from_str_seed(s: &str) -> Random {
        use sha2::{Digest, Sha512};
        let mut bytes = s.as_bytes().to_vec();
        bytes.extend_from_slice(&Sha512::digest(s.as_bytes()));
        // 32-bit words, least significant first, without the leading zero words of the integer
        let mut key: Vec<u32> = bytes
            .rchunks(4)
            .map(|c| c.iter().fold(0u32, |a, &b| (a << 8) | u32::from(b)))
            .collect();
        while key.len() > 1 && key.last() == Some(&0) {
            key.pop();
        }
        Self::init_by_array(&key)
    }

    fn from_seed_u128(mut n: u128) -> Random {
        let mut key = Vec::new();
        while n > 0 {
            key.push(n as u32);
            n >>= 32;
        }
        if key.is_empty() {
            key.push(0);
        }
        Self::init_by_array(&key)
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            for kk in 0..N {
                let y = (self.mt[kk] & 0x8000_0000) | (self.mt[(kk + 1) % N] & 0x7fff_ffff);
                let mag = if y & 1 == 1 { 0x9908_b0df } else { 0 };
                self.mt[kk] = self.mt[(kk + M) % N] ^ (y >> 1) ^ mag;
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `random()`: a float in [0, 1) with 53 random bits.
    pub fn random(&mut self) -> f64 {
        let a = self.next_u32() >> 5;
        let b = self.next_u32() >> 6;
        (f64::from(a) * 67108864.0 + f64::from(b)) * (1.0 / 9007199254740992.0)
    }

    /// `getrandbits(k)` for k <= 64.
    pub fn getrandbits(&mut self, k: u32) -> u64 {
        assert!(k <= 64, "getrandbits: at most 64 bits");
        if k == 0 {
            return 0;
        }
        if k <= 32 {
            return u64::from(self.next_u32() >> (32 - k));
        }
        let lo = u64::from(self.next_u32());
        let hi = u64::from(self.next_u32() >> (64 - k));
        lo | (hi << 32)
    }

    /// `_randbelow(n)`: an integer in [0, n).
    pub fn randbelow(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        let k = 64 - n.leading_zeros();
        loop {
            let r = self.getrandbits(k);
            if r < n {
                return r;
            }
        }
    }

    /// `randrange(start, stop)`.
    pub fn randrange(&mut self, start: i64, stop: i64) -> i64 {
        assert!(stop > start, "empty range for randrange");
        start + self.randbelow((stop - start) as u64) as i64
    }

    /// `randint(a, b)`: both ends included.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        self.randrange(a, b + 1)
    }

    /// `uniform(a, b)`.
    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }

    /// `choice(seq)`.
    pub fn choice<'a, T>(&mut self, seq: &'a [T]) -> &'a T {
        assert!(!seq.is_empty(), "choice from an empty sequence");
        &seq[self.randbelow(seq.len() as u64) as usize]
    }

    /// `shuffle(x)`.
    pub fn shuffle<T>(&mut self, x: &mut [T]) {
        for i in (1..x.len()).rev() {
            let j = self.randbelow(i as u64 + 1) as usize;
            x.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Random;

    #[test]
    fn string_seeds() {
        // r = random.Random('2026-11-19:3'); [r.random() for _ in range(2)]; random.Random('2026-11-19:10').choice(range(17))
        let mut r = Random::from_str_seed("2026-11-19:3");
        assert_eq!(r.random(), 0.4436923381830429);
        assert_eq!(r.random(), 0.6306404996600221);
        let v: Vec<i32> = (0..17).collect();
        assert_eq!(*Random::from_str_seed("2026-11-19:10").choice(&v), 6);
        assert_eq!(Random::from_str_seed("").random(), 0.9602256525641875);
    }

    #[test]
    fn same_as_cpython() {
        // python3 -c "import random; r = random.Random(42); print([r.random() for _ in range(3)], r.randint(1, 6),
        //             r.getrandbits(40), r.uniform(5, 9))"
        let mut r = Random::new(42);
        assert_eq!(r.random(), 0.6394267984578837);
        assert_eq!(r.random(), 0.025010755222666936);
        assert_eq!(r.random(), 0.27502931836911926);
        assert_eq!(r.randint(1, 6), 2);
        assert_eq!(r.getrandbits(40), 808053162473);
        assert_eq!(r.uniform(5.0, 9.0), 5.4099807046860295);
        let mut r = Random::new(0);
        assert_eq!(r.random(), 0.8444218515250481);
        // r = random.Random(2**40 + 7); x = list(range(10)); r.shuffle(x); r.choice('abcdef'); r.randrange(-5, 1000); r.getrandbits(64)
        let mut r = Random::new((1 << 40) + 7);
        let mut x: Vec<i32> = (0..10).collect();
        r.shuffle(&mut x);
        assert_eq!(x, [0, 5, 2, 3, 6, 1, 8, 4, 7, 9]);
        assert_eq!(*r.choice(&['a', 'b', 'c', 'd', 'e', 'f']), 'd');
        assert_eq!(r.randrange(-5, 1000), 448);
        assert_eq!(r.getrandbits(64), 15927155559309870078);
    }
}
