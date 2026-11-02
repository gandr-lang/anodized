#![cfg(not(any(anodized_discard_specs, anodized_hax)))]
#![cfg_attr(anodized_charon, feature(register_tool))]
#![cfg_attr(anodized_charon, register_tool(charon))]

use anodized::{spec, types::Spec};

#[derive(Clone, Copy)]
#[spec(maintains: self.0 > 0)]
struct Positive(i32);

#[spec]
struct Nested {
    values: Spec!([Positive; 2]),
}

#[spec]
fn value(input: Spec!(Positive)) -> i32 {
    input.0
}

#[test]
fn maintains_distinguishes_valid_and_invalid_values() {
    assert!(Positive(1).predicate());
    assert!(!Positive(0).predicate());
    assert!(!Positive(-1).predicate());
    assert!(
        Nested {
            values: [Positive(1), Positive(2)],
        }
        .predicate()
    );
    assert!(
        !Nested {
            values: [Positive(1), Positive(0)],
        }
        .predicate()
    );
}

#[test]
fn core_containers_preserve_refinements() {
    for number in [-1, 0, 1] {
        let mut item = Positive(number);
        let expected = number > 0;
        assert_eq!(<&Positive as Spec>::predicate(&&item), expected);
        assert_eq!(<&mut Positive as Spec>::predicate(&&mut item), expected);
        let items = [Positive(1), item];
        assert_eq!(items.predicate(), expected);
        assert_eq!(items.as_slice().predicate(), expected);
        assert_eq!(Some(item).predicate(), expected);
        assert_eq!(Result::<Positive, Positive>::Ok(item).predicate(), expected);
        assert_eq!(
            Result::<Positive, Positive>::Err(item).predicate(),
            expected
        );
        assert_eq!((Positive(1), item).predicate(), expected);
    }
    assert!(Option::<Positive>::None.predicate());
    assert!(<[Positive; 0]>::default().predicate());
    assert!(<[Positive; 0]>::default().as_slice().predicate());
    assert!(().predicate());
}

#[cfg(feature = "logic")]
#[test]
fn allocated_containers_preserve_refinements() {
    for number in [0, 1] {
        let expected = number > 0;
        let boxed = Box::new(Positive(number));
        assert_eq!(boxed.predicate(), expected);
        assert_eq!(vec![Positive(1), Positive(number)].predicate(), expected);
    }
    assert!(Vec::<Positive>::new().predicate());
}

#[test]
fn marked_input_obeys_runtime_check_configuration() {
    assert_eq!(value(Positive(1)), 1);
    let invalid = std::panic::catch_unwind(|| value(Positive(0)));
    assert_eq!(invalid.is_err(), cfg!(anodized_panic));
}
