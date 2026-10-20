//! Module for the internal use of `anodized_macros`.

use crate::types::Spec;

/// Apply the closure to an owned value and then recover it.
pub fn apply_keep<T, U>(closure: impl Fn(U) -> (T, U), value: U) -> (T, U) {
    closure(value)
}

/// Coerce the closure's input to a type based on a reference.
pub fn coerce_input<T>(_: impl Fn(T), _: &T) {}

/// Make the evaluated closure mutation-free by coercing it to `Fn`.
pub fn eval<T>(closure: impl Fn() -> T) -> T {
    closure()
}

/// Coerce the evaluated closure to `FnOnce`.
///
/// For details, see: <https://github.com/anodized-rs/anodized/issues/201>
pub fn eval_once<T>(closure: impl FnOnce() -> T) -> T {
    closure()
}

/// The type of a capture that is not evaluated, because no condition is.
pub struct Unevaluated<T>(core::marker::PhantomData<fn() -> T>);

impl<T> Clone for Unevaluated<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Unevaluated<T> {}

/// Type a capture's expression without evaluating it.
pub fn unevaluated<T>(_: impl Fn() -> T) -> Unevaluated<T> {
    Unevaluated(core::marker::PhantomData)
}

/// Stand in for an unevaluated capture inside a condition that is never
/// evaluated, so the condition still type-checks against the capture.
pub fn read_unevaluated<T>(_: Unevaluated<T>) -> T {
    unreachable!("a condition that reads an unevaluated capture is never evaluated")
}

/// Evaluate a type spec on a type that implements the `Spec` trait.
pub fn eval_type_spec<T: Spec + ?Sized>(value: &T) -> bool {
    value.predicate()
}
