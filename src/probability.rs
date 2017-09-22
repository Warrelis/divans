#![allow(unused)]
use core;
use core::clone::Clone;
pub type Prob = i16; // can be i32

// Common interface for CDF2 and CDF16, with optional methods.
pub trait BaseCDF {

    // the cardinality of symbols supported. Typical implementation values are 2 and 16.
    fn num_symbols() -> u8;

    // the cumulative distribution function evaluated at the given symbol.
    fn cdf(&self, symbol: u8) -> Prob;

    // the probability distribution function evaluated at the given symbol.
    fn pdf(&self, symbol: u8) -> Prob {
        debug_assert!(symbol < Self::num_symbols());
        if symbol == 0 {
            self.cdf(symbol)
        } else {
            self.cdf(symbol) - self.cdf(symbol - 1)
        }
    }

    // the maximum value relative to which cdf() and pdf() values should be normalized.
    fn max(&self) -> Prob;

    // the base-2 logarithm of max(), if available, to support bit-shifting.
    fn log_max(&self) -> Option<i8>;

    // returns true if used.
    fn used(&self) -> bool { false }

    // returns true if valid.
    fn valid(&self) -> bool { false }

    // returns the entropy of the current distribution.
    fn entropy(&self) -> f64 {
        let mut sum = 0.0f64;
        for i in 0..Self::num_symbols() {
            let v = self.pdf(i as u8);
            sum += if v == 0 { 0.0f64 } else {
                let v_f64 = (v as f64) / (self.max() as f64);
                v_f64 * (-v_f64.log2())
            };
        }
        sum
    }

    // These methods are optional because implementing them requires nontrivial bookkeeping.
    // Only CDFs that are intended for debugging should support them.
    fn num_samples(&self) -> Option<u32> { None }
    fn true_entropy(&self) -> Option<f64> { None }
    fn rolling_entropy(&self) -> Option<f64> { None }
    fn encoding_cost(&self) -> Option<f64> { None }
}

#[derive(Clone, Copy)]
pub struct CDF2 {
    counts: [u8; 2],
    pub prob: u8,
}

impl Default for CDF2 {
    fn default() -> Self {
        CDF2 {
            counts: [1, 1],
            prob: 128,
        }
    }
}

impl BaseCDF for CDF2 {
    fn num_symbols() -> u8 { 2 }
    fn cdf(&self, symbol: u8) -> Prob {
        match symbol {
            0 => self.prob as Prob,
            1 => 256 - self.prob as Prob,
            _ => { panic!("Symbol out of range"); }
        }
    }
    fn used(&self) -> bool {
        self.counts[0] != 1 || self.counts[1] != 1
    }
    fn max(&self) -> Prob {
        return 256;
    }
    fn log_max(&self) -> Option<i8> {
        return Some(8);
    }
}

impl CDF2 {
    pub fn blend(&mut self, symbol: bool, _speed: Speed) {
        let fcount = self.counts[0];
        let tcount = self.counts[1];
        debug_assert!(fcount != 0);
        debug_assert!(tcount != 0);

        let obs = if symbol == true {1} else {0};
        let overflow = self.counts[obs] == 0xff;
        self.counts[obs] = self.counts[obs].wrapping_add(1);
        if overflow {
            let not_obs = if symbol == true {0} else {1};
            let neverseen = self.counts[not_obs] == 1;
            if neverseen {
                self.counts[obs] = 0xff;
                self.prob = if symbol {0} else {0xff};
            } else {
                self.counts[0] = ((1 + (fcount as u16)) >> 1) as u8;
                self.counts[1] = ((1 + (tcount as u16)) >> 1) as u8;
                self.counts[obs] = 129;
                self.prob = (((self.counts[0] as u16) << 8) / (self.counts[0] as u16 + self.counts[1] as u16)) as u8;
            }
        } else {
            self.prob = (((self.counts[0] as u16) << 8) / (fcount as u16 + tcount as u16 + 1)) as u8;
        }
    }
}

pub enum Speed {
    GEOLOGIC,
    GLACIAL,
    MUD,
    SLOW,
    MED,
    FAST,
    PLANE,
    ROCKET,
}

pub trait CDF16: Sized + Default + Copy + BaseCDF {
    fn blend(&mut self, symbol: u8, dyn:Speed);

    // TODO: this convenience function should probably live elsewhere.
    fn float_array(&self) -> [f32; 16] {
        let mut ret = [0.0f32; 16];
        for i in 0..16 {
            ret[i] = (self.cdf(i as u8) as f32) / (self.max() as f32);
       }
        ret
    }
}

const CDF_BITS : usize = 15; // 15 bits
const CDF_MAX : Prob = 32767; // last value is implicitly 32768
const CDF_LIMIT : i64 = CDF_MAX as i64 + 1;

#[derive(Clone,Copy)]
pub struct BlendCDF16 {
    pub cdf: [Prob; 16],
    mix_rate: i32,
    count: i32,
}

impl Default for BlendCDF16 {
    fn default() -> Self {
        BlendCDF16 {
            cdf: [0; 16],
            mix_rate: (1 << 10) + (1 << 9),
            count: 0,
        }
    }
}

impl BaseCDF for BlendCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        for i in 0..16 {
            if self.cdf[i] > 0 {
                return true;
            }
        }
        false
    }
    fn max(&self) -> Prob {
        CDF_MAX as Prob
    }
    fn log_max(&self) -> Option<i8> {
        Some(15)
    }
    fn cdf(&self, symbol: u8) -> Prob {
        match symbol {
            15 => self.max(),
            _ => {
                // We want self.cdf[15] to be normalized to CDF_MAX, so take the difference to
                // be the latent bias term coming from a uniform distribution.
                let bias = CDF_MAX - self.cdf[15] as i16;
                debug_assert!(bias >= 16);
                self.cdf[symbol as usize] as Prob + (((bias as i32) * ((symbol + 1) as i32)) >> 4) as Prob
            }
        }
    }
    fn valid(&self) -> bool {
        for item in self.cdf.iter() {
            if *item < 0 || !(*item <= CDF_MAX) {
                return false;
            }
        }
        return true;
    }
}

impl CDF16 for BlendCDF16 {
    fn blend(&mut self, symbol:u8, speed: Speed) {
        self.count = self.count.wrapping_add(1);
        let _mix_rate = match speed {
                Speed::GEOLOGIC => 32,
                Speed::GLACIAL => 64,
                Speed::MUD => 160,
                Speed::SLOW => 512,
                Speed::MED => 192,
                Speed::FAST => 256,
                Speed::PLANE => 384,
                Speed::ROCKET => 1100,
        };
        self.cdf = mul_blend(self.cdf, symbol, self.mix_rate, (self.count & 0xf) << (BLEND_FIXED_POINT_PRECISION - 4));
        // NOTE(jongmin): geometrically decay mix_rate until it dips below 1 << 7;
        self.mix_rate -= self.mix_rate >> 7;

        // Reduce the weight of bias in the first few iterations.
        if self.cdf[15] < (CDF_MAX - 16) - (self.cdf[15] >> 1) {
            for i in 0..16 {
                self.cdf[i] += self.cdf[i] >> 1;
            }
        }
        debug_assert!(self.cdf[15] <= CDF_MAX - 16);
    }
}

#[derive(Clone,Copy)]
pub struct ExternalProbCDF16 {
    pub cdf: [Prob; 16],
    pub nibble: usize,
    maxp: Prob,
}

impl Default for ExternalProbCDF16 {
    fn default() -> Self {
        ExternalProbCDF16 {
            cdf: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            nibble: 0,
            maxp:  (1<<15 - 1)
        }
    }
}

impl ExternalProbCDF16 {
    fn init<T: BaseCDF>(&mut self, nibble: u8, prob: f64, mix: T) {
        self.nibble = nibble as usize;
        let p = mix.cdf(nibble);
        let m = mix.max(); 
        let r = ((((p as f64 / (m as f64)) + prob)/2f64) * (self.maxp as f64)) as Prob;
        let i = (self.maxp - r) / 15;
        for v in self.cdf.iter_mut() {
            *v = i;
        }
        self.cdf[self.nibble] = r
    }
}

impl BaseCDF for ExternalProbCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        self.entropy() != Self::default().entropy()
    }
    fn max(&self) -> Prob {
        self.maxp
    }
    fn log_max(&self) -> Option<i8> { None }
    fn cdf(&self, symbol: u8) -> Prob {
        assert!(symbol as usize == self.nibble);
        self.cdf[symbol as usize]
    }
    fn valid(&self) -> bool {
        return true;
    }
}

impl CDF16 for ExternalProbCDF16 {
    fn blend(&mut self, symbol: u8, speed: Speed) {
        return;
    }
}

#[derive(Clone,Copy)]
pub struct FrequentistCDF16 {
    pub cdf: [Prob; 16]
}

impl Default for FrequentistCDF16 {
    fn default() -> Self {
        FrequentistCDF16 {
            cdf: [4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64]
        }
    }
}

#[allow(unused)]
macro_rules! each16{
    ($src0: expr, $func: expr) => {
    [$func($src0[0]),
     $func($src0[1]),
     $func($src0[2]),
     $func($src0[3]),
     $func($src0[4]),
     $func($src0[5]),
     $func($src0[6]),
     $func($src0[7]),
     $func($src0[8]),
     $func($src0[9]),
     $func($src0[10]),
     $func($src0[11]),
     $func($src0[12]),
     $func($src0[13]),
     $func($src0[14]),
     $func($src0[15]),
    ]
    }
}
#[allow(unused)]
macro_rules! set1 {
    ($src: expr, $val: expr) =>{
        [$val; 16]
    }
}
macro_rules! each16bin {
    ($src0 : expr, $src1 : expr, $func: expr) => {
    [$func($src0[0], $src1[0]),
           $func($src0[1], $src1[1]),
           $func($src0[2], $src1[2]),
           $func($src0[3], $src1[3]),
           $func($src0[4], $src1[4]),
           $func($src0[5], $src1[5]),
           $func($src0[6], $src1[6]),
           $func($src0[7], $src1[7]),
           $func($src0[8], $src1[8]),
           $func($src0[9], $src1[9]),
           $func($src0[10], $src1[10]),
           $func($src0[11], $src1[11]),
           $func($src0[12], $src1[12]),
           $func($src0[13], $src1[13]),
           $func($src0[14], $src1[14]),
           $func($src0[15], $src1[15])]
    }
}

fn srl(a:Prob) -> Prob {
    a >> 1
}

impl BaseCDF for FrequentistCDF16 {
    fn num_symbols() -> u8 { 16 }
    fn used(&self) -> bool {
        self.entropy() != Self::default().entropy()
    }
    fn max(&self) -> Prob {
        self.cdf[15]
    }
    fn log_max(&self) -> Option<i8> { None }
    fn cdf(&self, symbol: u8) -> Prob {
        self.cdf[symbol as usize]
    }
    fn valid(&self) -> bool {
        let mut prev = 0;
        for item in self.cdf.split_at(15).0.iter() {
            if *item <= prev {
                return false;
            }
            prev = *item;
        }
        return true;
    }
}

impl CDF16 for FrequentistCDF16 {
    fn blend(&mut self, symbol: u8, speed: Speed) {
        const CDF_BIAS : [Prob;16] = [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16];
        let increment : Prob =
            match speed {
                Speed::GEOLOGIC => 1,
                Speed::GLACIAL => 4,
                Speed::MUD => 16,
                Speed::SLOW => 32,
                Speed::MED => 48,
                Speed::FAST => 96,
                Speed::PLANE => 128,
                Speed::ROCKET => 384,
            };
        for i in (symbol as usize)..16 {
            self.cdf[i] = self.cdf[i].wrapping_add(increment);
        }
        let limit: Prob = 32767 - 16 - 384 /* XXX: max possible increment */;
        if self.cdf[15] >= limit {
            for i in 0..16 {
                self.cdf[i] = self.cdf[i].wrapping_add(CDF_BIAS[i]).wrapping_sub(self.cdf[i].wrapping_add(CDF_BIAS[i]) >> 2);
            }
        }
    }
}

#[allow(unused)]
fn gt(a:Prob, b:Prob) -> Prob {
    (-((a > b) as i64)) as Prob
}
#[allow(unused)]
fn gte(a:Prob, b:Prob) -> Prob {
    (-((a >= b) as i64)) as Prob
}
#[allow(unused)]
fn gte_bool(a:Prob, b:Prob) -> Prob {
    (a >= b) as Prob
}

fn and(a:Prob, b:Prob) -> Prob {
    a & b
}
fn add(a:Prob, b:Prob) -> Prob {
    a.wrapping_add(b)
}

const BLEND_FIXED_POINT_PRECISION : i8 = 15;

pub fn mul_blend(baseline: [Prob;16], symbol: u8, blend : i32, bias : i32) -> [Prob;16] {
    const SCALE :i32 = 1i32 << BLEND_FIXED_POINT_PRECISION;
    let to_blend = to_blend_lut(symbol);
    let mut epi32:[i32;8] = [to_blend[0] as i32,
                             to_blend[1] as i32,
                             to_blend[2] as i32,
                             to_blend[3] as i32,
                             to_blend[4] as i32,
                             to_blend[5] as i32,
                             to_blend[6] as i32,
                             to_blend[7] as i32];
    let scale_minus_blend = SCALE - blend;
    for i in 0..8 {
        epi32[i] *= blend;
        epi32[i] += baseline[i] as i32 * scale_minus_blend + bias;
        epi32[i] >>= BLEND_FIXED_POINT_PRECISION;
    }
    let mut retval : [Prob;16] =[epi32[0] as Prob,
                                 epi32[1] as Prob,
                                 epi32[2] as Prob,
                                 epi32[3] as Prob,
                                 epi32[4] as Prob,
                                 epi32[5] as Prob,
                                 epi32[6] as Prob,
                                 epi32[7] as Prob,
                                 0,0,0,0,0,0,0,0];
    let mut epi32:[i32;8] = [to_blend[8] as i32,
                             to_blend[9] as i32,
                             to_blend[10] as i32,
                             to_blend[11] as i32,
                             to_blend[12] as i32,
                             to_blend[13] as i32,
                             to_blend[14] as i32,
                             to_blend[15] as i32];
    for i in 8..16 {
        epi32[i - 8] *= blend;
        epi32[i - 8] += baseline[i] as i32 * scale_minus_blend + bias;
        retval[i] = (epi32[i - 8] >> BLEND_FIXED_POINT_PRECISION) as Prob;
    }
    retval
}

fn to_blend(symbol: u8) -> [Prob;16] {
    // The returned distribution has a max of DEL = CDF_MAX - 16, which guarantees that
    // by mixing only such distributions, we'll have at least 16 as the bias weight,
    // which is required to guarantee nonzero PDF everywhere.
    const CDF_INDEX : [Prob;16] = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15];
    const DEL: Prob = CDF_MAX - 16;
    let symbol16 = [symbol as Prob; 16];
    let delta16 = [DEL; 16];
    let mask_symbol = each16bin!(CDF_INDEX, symbol16, gte);
    let add_mask = each16bin!(delta16, mask_symbol, and);
    add_mask
}

fn to_blend_lut(symbol: u8) -> [Prob;16] {
    const DEL: Prob = CDF_MAX - 16;
    static CDF_SELECTOR : [[Prob;16];16] = [
        [DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,DEL,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,DEL,DEL as Prob],
        [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,DEL as Prob]];
    CDF_SELECTOR[symbol as usize]
}

#[cfg(feature="debug_entropy")]
#[derive(Clone,Copy,Default)]
pub struct DebugWrapperCDF16<Cdf16: CDF16> {
    pub cdf: Cdf16,
    pub counts: [u32; 16],
    cost: f64,
    rolling_entropy_sum: f64
}

#[cfg(feature="debug_entropy")]
impl<Cdf16> CDF16 for DebugWrapperCDF16<Cdf16> where Cdf16: CDF16 {
    fn blend(&mut self, symbol: u8, speed: Speed) {
        self.counts[symbol as usize] += 1;
        let p = self.cdf.pdf(symbol) as f64 / self.cdf.max() as f64;
        self.cost += -p.log2();
        match self.true_entropy() {
            None => {},
            Some(e) => { self.rolling_entropy_sum += e; }
        }
        self.cdf.blend(symbol, speed);
    }
    fn float_array(&self) -> [f32; 16] { self.cdf.float_array() }
}

#[cfg(feature="debug_entropy")]
impl<Cdf16> BaseCDF for DebugWrapperCDF16<Cdf16> where Cdf16: CDF16 + BaseCDF {
    fn num_symbols() -> u8 { 16 }
    fn cdf(&self, symbol: u8) -> Prob { self.cdf.cdf(symbol) }
    fn pdf(&self, symbol: u8) -> Prob { self.cdf.pdf(symbol) }
    fn max(&self) -> Prob { self.cdf.max() }
    fn log_max(&self) -> Option<i8> { self.cdf.log_max() }
    fn entropy(&self) -> f64 { self.cdf.entropy() }
    fn valid(&self) -> bool { self.cdf.valid() }
    fn used(&self) -> bool {
        self.num_samples().unwrap() > 0
    }

    fn num_samples(&self) -> Option<u32> {
        let mut sum : u32 = 0;
        for i in 0..16 {
            sum += self.counts[i];
        }
        Some(sum)
    }
    fn true_entropy(&self) -> Option<f64> {
        let num_samples = self.num_samples().unwrap();
        if num_samples > 0 {
            let mut sum : f64 = 0.0;
            for i in 0..16 {
                sum += if self.counts[i] == 0 { 0.0f64 } else {
                    let p = (self.counts[i] as f64) / (num_samples as f64);
                    p * (-p.log2())
                };
            }
            Some(sum)
        } else {
            None
        }
    }
    fn rolling_entropy(&self) -> Option<f64> {
        match self.num_samples() {
            None => None,
            Some(n) => Some(self.rolling_entropy_sum / n as f64)
        }
    }
    fn encoding_cost(&self) -> Option<f64> {
        Some(self.cost)
    }
}

#[cfg(feature="debug_entropy")]
impl<Cdf16> DebugWrapperCDF16<Cdf16> where Cdf16: CDF16 {
    fn new(cdf: Cdf16) -> Self {
        DebugWrapperCDF16::<Cdf16> {
            cdf: cdf,
            counts: [0; 16],
            cost: 0.0,
            rolling_entropy_sum: 0.0
        }
    }
}

mod test {
    use super::{BaseCDF, BlendCDF16, CDF16, FrequentistCDF16, Speed};

    #[test]
    fn test_blend_lut() {
        for i in 0..16 {
            let a = super::to_blend(i as u8);
            let b = super::to_blend_lut(i as u8);
            for j in 0..16 {
                assert_eq!(a[j], b[j]);
            }
        }
    }

    #[allow(unused)]
    const RAND_MAX : u32 = 32767;
    #[allow(unused)]
    fn simple_rand(state: &mut u64) -> u32 {
        *state = (*state).wrapping_mul(1103515245).wrapping_add(12345);
        return ((*state / 65536) as u32 % (RAND_MAX + 1)) as u32;
    }

    #[allow(unused)]
    #[cfg(test)]
    fn test_random_cdf<C: CDF16>(mut prob_state: C,
                                 rand_table : [(u32, u32); 16],
                                 num_trials: usize) -> C {
        let mut cutoffs : [u32; 16] = [0; 16];
        let mut sum_prob : f32 = 0.0f32;
        for i in 0..16 {
            sum_prob += (rand_table[i].0 as f32) / (rand_table[i].1 as f32);
            cutoffs[i] = (((RAND_MAX + 1) as f32) * sum_prob).round() as u32;
        }
        assert_eq!(cutoffs[15], RAND_MAX + 1);
        // make sure we have all probability taken care of
        let mut seed = 1u64;
        for i in 0..num_trials {
            let rand_num = simple_rand(&mut seed) as u32;
            for j in 0..16 {
                if rand_num < cutoffs[j] {
                    // we got an j as the next symbol
                    prob_state.blend(j as u8, Speed::MED);
                    assert!(prob_state.valid());
                    break;
                }
                assert!(j != 15); // should have broken
            }
        }
        for i in 0..16 {
            let actual = (prob_state.pdf(i as u8) as f32) / (prob_state.max() as f32);
            let expected = (rand_table[i].0 as f32) / (rand_table[i].1 as f32);
            let abs_delta = (expected - actual).abs();
            let rel_delta = abs_delta / expected;  // may be nan
            // TODO: These bounds should be tightened.
            assert!(rel_delta < 0.15f32 || abs_delta < 0.014f32);
        }
        prob_state
    }
    #[test]
    fn test_stationary_probability_blend_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(BlendCDF16::default(),
                        [(0,1), (0,1), (1,16), (0,1),
                         (1,32), (1,32), (0,1), (0,1),
                         (1,8), (0,1), (0,1), (0,1),
                         (1,5), (1,5), (1,5), (3,20)],
                        1000000);
    }
    #[test]
    fn test_stationary_probability_frequentist_cdf() {
        let rm = RAND_MAX as u32;
        test_random_cdf(FrequentistCDF16::default(),
                        [(0,1), (0,1), (1,16), (0,1),
                         (1,32), (1,32), (0,1), (0,1),
                         (1,8), (0,1), (0,1), (0,1),
                         (1,5), (1,5), (1,5), (3,20)],
                        1000000);
    }
    #[cfg(feature="debug_entropy")]
    #[test]
    fn test_stationary_probability_debug_cdf() {
        let rm = RAND_MAX as u32;
        let wrapper_cdf = test_random_cdf(super::DebugWrapperCDF16::<FrequentistCDF16>::default(),
                                          [(0,1), (0,1), (1,16), (0,1),
                                           (1,32), (1,32), (0,1), (0,1),
                                           (1,8), (0,1), (0,1), (0,1),
                                           (1,5), (1,5), (1,5), (3,20)],
                                          1000000);
        assert!(wrapper_cdf.num_samples().is_some());
        assert_eq!(wrapper_cdf.num_samples().unwrap(), 1000000);
    }
    #[test]
    fn test_blend_cdf_nonzero_pdf() {
        // This is a regression test
        let mut prob_state = BlendCDF16::default();
        for n in 0..1000000 {
            prob_state.blend(15, Speed::MED);
        }
        for i in 0..14 {
            assert!(prob_state.pdf(i) > 0);
        }
    }
}
