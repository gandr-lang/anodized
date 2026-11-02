//! Constant evaluation and runtime checks share the same specified functions.
#![cfg(not(any(anodized_print, anodized_try, anodized_embed_specs, anodized_charon)))]

use anodized::spec;

// Primitive signatures here exercise Rust's built-in const operations directly.
/// Increment within the admitted range.
///
/// # Specification
/// - requires: the input is below ten.
/// - ensures: returns the input plus one.
/// - panics: an enabled precondition rejects inputs at or above ten.
#[deny(
    clippy::panic,
    clippy::manual_assert,
    clippy::nonminimal_bool,
    clippy::missing_panics_doc
)]
#[spec(requires: value < 10, ensures: |output| output == value.saturating_add(1))]
pub const fn increment(value: u32) -> u32 {
    value.saturating_add(1)
}

/// Keep a mutable input below ten while advancing it.
///
/// # Specification
/// - requires: the input is below ten.
/// - ensures: advances the input by one and leaves it below ten.
/// - panics: enabled checks reject entry or exit outside the admitted range.
#[spec(maintains: *value < 10)]
const fn advance(value: &mut u32) {
    *value = value.saturating_add(1);
}

/// Advance a local value during constant evaluation.
///
/// # Specification
/// - ensures: returns five.
/// - panics: none.
const fn advanced() -> u32 {
    let mut value = 4;
    advance(&mut value);
    value
}

/// An owned, non-Copy output.
struct Pair(u32, u32);

/// Return an owned output through two independently scoped patterns.
///
/// # Specification
/// - ensures: both components are equal, with the second one at least four.
/// - panics: enabled checks reject unequal components or values below four.
#[spec(ensures: |Pair(left, right)| left == right, ensures: |Pair(_, ref right)| *right >= 4)]
const fn pair(value: u32) -> Pair {
    Pair(value, value)
}

/// Return through an early exit or through a nested function.
///
/// # Specification
/// - ensures: returns four on both paths.
/// - panics: none.
#[spec(ensures: |output| output == 4)]
const fn early(value: bool) -> u32 {
    if value {
        return 4;
    }
    /// Return from a scope independent of the enclosing specification.
    ///
    /// # Specification
    /// - ensures: returns four.
    /// - panics: none.
    const fn nested() -> u32 {
        4
    }
    nested()
}

/// Deliberately violate a postcondition through an explicit return.
///
/// # Specification
/// - ensures: returns a value greater than zero.
/// - panics: enabled checks reject the zero output.
#[spec(ensures: |output| output > 0)]
const fn bad_exit(value: bool) -> u32 {
    if value {
        return 0;
    }
    1
}

/// Keep a disabled condition type-checked without evaluating it.
///
/// # Specification
/// - ensures: returns seven; the cfg-disabled check never panics.
/// - panics: none.
#[spec(#[cfg(any())] requires: forbidden_predicate(), ensures: |output| output == 7)]
const fn gated() -> u32 {
    7
}

/// Fail if a disabled predicate is accidentally evaluated.
///
/// # Specification
/// - panics: always.
#[cfg(not(anodized_discard_specs))]
const fn forbidden_predicate() -> bool {
    panic!("disabled predicate evaluated")
}

/// Preserve constness for associated methods inside an annotated impl.
struct Counter(u32);

#[spec]
impl Counter {
    /// Project the stored number.
    ///
    /// # Specification
    /// - ensures: returns the stored number.
    /// - panics: none.
    #[spec(ensures: |output| output == self.0)]
    const fn value(&self) -> u32 {
        self.0
    }
}

const INCREMENTED: u32 = increment(4);
const ADVANCED: u32 = advanced();
const PAIR: Pair = pair(4);
const EARLY: u32 = early(true);
const NESTED: u32 = early(false);
const GATED: u32 = gated();
const METHOD: u32 = Counter(9).value();

#[test]
fn const_items_and_runtime_agree() {
    assert_eq!((INCREMENTED, ADVANCED), (5, 5));
    assert_eq!((PAIR.0, PAIR.1), (4, 4));
    assert_eq!((EARLY, NESTED, GATED, METHOD), (4, 4, 7, 9));
    assert_eq!(increment(0), 1);
    assert_eq!(increment(9), 10);
    assert_eq!((early(true), early(false)), (4, 4));
    let Pair(left, right) = pair(5);
    assert_eq!((left, right), (5, 5));
    assert_eq!(gated(), 7);
    assert_eq!(Counter(8).value(), 8);
}

#[test]
fn violations_reach_the_correct_boundary() {
    let enforced = cfg!(all(anodized_panic, not(anodized_discard_specs)));
    assert_eq!(
        std::panic::catch_unwind(|| increment(10)).is_err(),
        enforced
    );
    assert_eq!(
        std::panic::catch_unwind(|| bad_exit(true)).is_err(),
        enforced
    );
    assert_eq!(std::panic::catch_unwind(|| pair(3)).is_err(), enforced);
    assert_eq!(
        std::panic::catch_unwind(|| {
            let mut value = 9;
            advance(&mut value);
            value
        })
        .is_err(),
        enforced
    );
    assert_eq!(
        std::panic::catch_unwind(|| {
            let mut value = 10;
            advance(&mut value);
            value
        })
        .is_err(),
        enforced
    );
    assert_eq!(bad_exit(false), 1);
}

#[cfg(not(anodized_panic))]
#[test]
fn disabled_checks_do_not_evaluate_predicates() {
    /// Evaluate only the body when checking is disabled.
    ///
    /// # Specification
    /// - ensures: returns three with instrumentation disabled.
    /// - panics: an enabled precondition always panics.
    #[spec(requires: forbidden_predicate(), ensures: forbidden_predicate())]
    const fn disabled() -> u32 {
        3
    }
    const VALUE: u32 = disabled();
    assert_eq!((VALUE, disabled()), (3, 3));
}

/// Carry an owned, non-Copy value with drop glue across a const postcondition.
///
/// # Specification
/// - requires: the input contains at most two bytes.
/// - ensures: returns the input unchanged and retains at most two bytes.
/// - panics: an enabled postcondition rejects longer inputs.
#[spec(ensures: |ref output| { output.len() <= 2 })]
const fn owned(values: Vec<u8>) -> Vec<u8> {
    values
}

#[test]
fn owned_outputs_need_no_const_destructor() {
    const EMPTY: Vec<u8> = owned(Vec::new());
    assert_eq!(EMPTY, Vec::<u8>::new());
    assert_eq!(owned(vec![7, 9]), vec![7, 9]);
    assert_eq!(
        std::panic::catch_unwind(|| owned(vec![1, 2, 3])).is_err(),
        cfg!(all(anodized_panic, not(anodized_discard_specs))),
    );
}
