//! Property tests for the fixed-width code newtypes.
//!
//! These types are `Copy` wrappers over byte arrays with hand-written parsing,
//! which is exactly the shape where an off-by-one hides for years. The
//! properties are simple on purpose: anything we can display, we must be able to
//! parse back to the same value, and anything malformed must be rejected rather
//! than silently truncated.

use diurn_mic::{CountryCode, Lei, MarketCategory, Mic};
use proptest::prelude::*;

/// Exactly the alphabet ISO uses for MICs and LEIs.
fn alnum() -> impl Strategy<Value = char> {
    prop_oneof![
        proptest::char::range('A', 'Z'),
        proptest::char::range('0', '9'),
    ]
}

fn code_of(len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(alnum(), len..=len).prop_map(|v| v.into_iter().collect())
}

proptest! {
    #[test]
    fn mic_round_trips(s in code_of(4)) {
        let mic = Mic::new(&s).expect("valid alphanumeric code");
        prop_assert_eq!(mic.as_str(), s.as_str());
        prop_assert_eq!(Mic::new(mic.as_str()).unwrap(), mic);
        prop_assert_eq!(mic.to_string().parse::<Mic>().unwrap(), mic);
    }

    /// Case is normalised, so a lowercase spelling is the same value.
    #[test]
    fn mic_is_case_insensitive(s in code_of(4)) {
        prop_assert_eq!(
            Mic::new(&s.to_lowercase()).unwrap(),
            Mic::new(&s).unwrap()
        );
    }

    #[test]
    fn lei_round_trips(s in code_of(20)) {
        let lei = Lei::new(&s).expect("valid alphanumeric code");
        prop_assert_eq!(lei.as_str(), s.as_str());
        prop_assert_eq!(Lei::new(lei.as_str()).unwrap(), lei);
    }

    #[test]
    fn country_round_trips(s in proptest::collection::vec(proptest::char::range('A', 'Z'), 2..=2)
        .prop_map(|v| v.into_iter().collect::<String>()))
    {
        let cc = CountryCode::new(&s).expect("valid alphabetic code");
        prop_assert_eq!(cc.as_str(), s.as_str());
        prop_assert_eq!(CountryCode::new(cc.as_str()).unwrap(), cc);
    }

    /// Unknown market categories preserve their code verbatim, which is what
    /// lets an older build of this crate round-trip a code ISO added later.
    #[test]
    fn market_category_round_trips(s in code_of(4)) {
        let c = MarketCategory::new(&s).expect("valid alphanumeric code");
        prop_assert_eq!(c.as_str(), s.as_str());
        prop_assert_eq!(MarketCategory::new(c.as_str()).unwrap(), c);
    }

    /// Any length but four is rejected — never truncated or padded.
    #[test]
    fn mic_rejects_wrong_length(s in "[A-Z0-9]{0,12}".prop_filter("not 4", |s| s.len() != 4)) {
        prop_assert!(Mic::new(&s).is_err());
    }

    #[test]
    fn lei_rejects_wrong_length(s in "[A-Z0-9]{0,40}".prop_filter("not 20", |s| s.len() != 20)) {
        prop_assert!(Lei::new(&s).is_err());
    }

    /// Anything outside the alphabet is rejected, and parsing never panics —
    /// including on multibyte input, where byte and char indices diverge.
    #[test]
    fn parsers_never_panic_on_arbitrary_input(s in ".{0,32}") {
        let _ = Mic::new(&s);
        let _ = Lei::new(&s);
        let _ = CountryCode::new(&s);
        let _ = MarketCategory::new(&s);
    }

    #[test]
    fn mic_rejects_non_alphanumeric(
        s in "[A-Z0-9]{3}",
        bad in prop_oneof![Just('-'), Just(' '), Just('/'), Just('_'), Just('.')],
    ) {
        let trailing = format!("{s}{bad}");
        let leading = format!("{bad}{s}");
        prop_assert!(Mic::new(&trailing).is_err());
        prop_assert!(Mic::new(&leading).is_err());
    }

    /// Ordering on the newtype must agree with ordering on the string form,
    /// since the registry sorts by MIC for stable output.
    #[test]
    fn mic_ordering_matches_string_ordering(a in code_of(4), b in code_of(4)) {
        let (ma, mb) = (Mic::new(&a).unwrap(), Mic::new(&b).unwrap());
        prop_assert_eq!(ma.cmp(&mb), a.cmp(&b));
    }
}
