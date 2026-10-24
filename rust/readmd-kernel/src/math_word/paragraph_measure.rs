//! Exact, bounded output measures. The author's lexical value lives only in
//! WordParagraphMeasure; no rounded twip or cached float accompanies this view.
use std::num::NonZeroU128;

const DOMAIN: &str = "unsupported-indent-domain-or-axis";
const PRECISION: &str = "unsupported-measure-precision";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RationalPoints {
    numerator: u128,
    denominator: NonZeroU128,
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 { let remainder = a % b; a = b; b = remainder; }
    a
}

impl RationalPoints {
    // Construction is confined to exact integer arithmetic in this module.
    // The finite consumer domain is the historical 0..31680 twips, expressed
    // as 0..1584 points. Division/remainder avoids an overflowing bound product.
    fn checked(numerator: u128, denominator: u128) -> Result<Self, &'static str> {
        let denominator = NonZeroU128::new(denominator).ok_or(PRECISION)?;
        let whole = numerator / denominator.get();
        if whole > 1584 || (whole == 1584 && numerator % denominator.get() != 0) {
            return Err(DOMAIN);
        }
        let divisor = gcd(numerator, denominator.get());
        Ok(Self { numerator: numerator / divisor,
            denominator: NonZeroU128::new(denominator.get() / divisor).ok_or(PRECISION)? })
    }

    fn decimal(raw: &str, fraction_allowed: bool, plus_allowed: bool,
        unit_numerator: u128, unit_denominator: u128) -> Result<Self, &'static str>
    {
        let negative = raw.starts_with('-');
        let number = if negative { &raw[1..] } else if let Some(value) = raw.strip_prefix('+') {
            if !plus_allowed { return Err(DOMAIN); }
            value
        } else { raw };
        let (whole, fraction) = match number.split_once('.') {
            Some((whole, fraction)) if fraction_allowed && !fraction.is_empty() => (whole, fraction),
            Some(_) => return Err(DOMAIN),
            None => (number, ""),
        };
        if whole.is_empty() || !whole.bytes().chain(fraction.bytes()).all(|c| c.is_ascii_digit()) {
            return Err(DOMAIN);
        }
        // Long padding and signed zero do not spend rational capacity. In
        // particular, an arbitrarily padded zero never becomes Unsupported.
        let whole = whole.trim_start_matches('0');
        let fraction = fraction.trim_end_matches('0');
        if whole.is_empty() && fraction.is_empty() { return Self::checked(0, 1); }
        if negative { return Err(DOMAIN); }
        let numerator = whole.bytes().chain(fraction.bytes()).try_fold(0u128, |value, c| {
            value.checked_mul(10)?.checked_add(u128::from(c - b'0'))
        }).ok_or(PRECISION)?;
        let exponent = u32::try_from(fraction.len()).map_err(|_| PRECISION)?;
        let denominator = 10u128.checked_pow(exponent).ok_or(PRECISION)?;
        let decimal_divisor = gcd(numerator, denominator);
        let numerator = numerator / decimal_divisor;
        let denominator = denominator / decimal_divisor;
        // Cross-cancel before multiplication. Neither range admission nor
        // precision admission uses a float, a rounded twip, or truncation.
        let left = gcd(numerator, unit_denominator);
        let right = gcd(unit_numerator, denominator);
        let numerator = (numerator / left).checked_mul(unit_numerator / right).ok_or(PRECISION)?;
        let denominator = (denominator / right).checked_mul(unit_denominator / left).ok_or(PRECISION)?;
        Self::checked(numerator, denominator)
    }

    pub(super) fn twips(raw: &str) -> Result<Self, &'static str> {
        Self::decimal(raw.trim_matches([' ', '\t', '\r', '\n']), false, true, 1, 20)
    }

    pub(super) fn unit(raw: &str) -> Result<Self, &'static str> {
        // Word's Office2010 signed-measure union uses these exact lowercase
        // suffixes. pc and pi are the two accepted pica spellings (12pt).
        for (unit, numerator, denominator) in [
            ("mm", 360, 127), ("cm", 3600, 127), ("in", 72, 1),
            ("pt", 1, 1), ("pc", 12, 1), ("pi", 12, 1),
        ] {
            if let Some(number) = raw.strip_suffix(unit) {
                return Self::decimal(number, true, false, numerator, denominator);
            }
        }
        Err(DOMAIN)
    }

    /// Conversion happens only after exact validation and range admission.
    pub(crate) fn points(self) -> f64 {
        self.numerator as f64 / self.denominator.get() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_reduce_to_exact_points_including_noninteger_twips() {
        for raw in ["25.4mm", "2.54cm", "1in", "72pt", "6pc", "6pi"] {
            assert_eq!(RationalPoints::unit(raw).unwrap(), RationalPoints::twips("1440").unwrap(), "{raw}");
        }
        let mm = RationalPoints::unit("1mm").unwrap();
        assert_eq!((mm.numerator, mm.denominator.get()), (360, 127));
        assert_ne!(mm, RationalPoints::twips("57").unwrap());
        assert_eq!(RationalPoints::unit("0.1cm").unwrap(), mm);
    }

    #[test]
    fn exact_range_checks_precede_float_rounding_at_the_upper_boundary() {
        for raw in ["558.8mm", "55.88cm", "22in", "1584pt", "132pc", "132pi"] {
            assert_eq!(RationalPoints::unit(raw).unwrap().points(), 1584.);
        }
        let above = "1584.000000000000000000000000000001";
        assert_eq!(above.parse::<f64>().unwrap(), 1584.);
        assert_eq!(RationalPoints::unit(&format!("{above}pt")), Err(DOMAIN));
        assert!(RationalPoints::unit("1583.999999999999999999999999999999pt").is_ok());
        assert_eq!(RationalPoints::twips("31681"), Err(DOMAIN));
    }

    #[test]
    fn decimal_reduction_precedes_unit_multiplication_at_u128_capacity() {
        let raw = format!("1.{}5in", "0".repeat(37));
        let value = RationalPoints::unit(&raw).unwrap();
        assert_eq!(value.numerator, (10u128.pow(38) / 5 + 1) * 9);
        assert_eq!(value.denominator.get(), 10u128.pow(38) / 40);
        assert!((value.points() - 72.).abs() < 1e-12);
        assert_ne!(value, RationalPoints::unit("1in").unwrap());
    }

    #[test]
    fn zero_padding_is_free_but_nonzero_precision_and_overflow_are_explicit() {
        for raw in [format!("-000.{}mm", "0".repeat(4096)), format!("{}.0000pc", "0".repeat(4096))] {
            assert_eq!(RationalPoints::unit(&raw).unwrap().points(), 0.);
        }
        assert_eq!(RationalPoints::unit(&format!("0001.{}mm", "0".repeat(4096))).unwrap(), RationalPoints::unit("1mm").unwrap());
        for raw in [format!("0.{}1mm", "0".repeat(4096)), format!("{}pt", "9".repeat(4096))] {
            assert_eq!(RationalPoints::unit(&raw), Err(PRECISION));
        }
        assert_eq!(RationalPoints::unit("-0.0001pt"), Err(DOMAIN));
        assert_eq!(RationalPoints::twips(" -000 ").unwrap().points(), 0.);
        assert_eq!(RationalPoints::unit("+1pt"), Err(DOMAIN));
    }
}
