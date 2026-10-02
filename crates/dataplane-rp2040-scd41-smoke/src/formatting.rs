//! Allocation-free two-decimal sensor output, truncated toward zero.
use core::fmt;

pub struct F32_2(pub f32);

impl fmt::Display for F32_2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scaled = (self.0 * 100.0) as i32;
        if scaled < 0 {
            f.write_str("-")?;
        }
        let magnitude = scaled.unsigned_abs();
        write!(f, "{}.{:02}", magnitude / 100, magnitude % 100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subzero_temperatures_keep_their_sign() {
        for (value, expected) in [(-0.5, "-0.50"), (-1.25, "-1.25"), (0.0, "0.00"), (23.5, "23.50")] {
            assert_eq!(std::format!("{}", F32_2(value)), expected);
        }
    }
}
