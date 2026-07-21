//! The constructor symbol of every hand-written projection must be the kebab of
//! the Rust name it stands for.
//!
//! # Why this exists
//!
//! The 116 macro-generated impls take their constructor symbol from
//! `kebab(stringify!($ty))`, so it cannot be wrong. The hand-written impls spell
//! it as a string literal — `Sexp::sym("measured-fraction")` — and a typo there is
//! **invisible to every other test in the suite**. `project` and `parse` would
//! agree with each other on `"measured-fracton"`, the value would round-trip, the
//! text would be self-consistent, and it would still not be the projection
//! `req:textproj:value-projection` clause 1 and 3 specify.
//!
//! `Debug` is derived on all of these, and its output begins with the variant or
//! struct name. So the name is recoverable at runtime and can be compared against
//! the symbol the projection actually emits. That closes the gap.
//!
//! What it does **not** close: field *order* inside a variant. See
//! `textvalue_roundtrip.rs`.

use epiphany_core::textvalue::{kebab, Sexp, TextValue};

/// Asserts that `value`'s projection is headed by the kebab of its Rust name.
///
/// Applies to a fieldless variant or zero-field struct (a bare symbol) and to a
/// struct or a variant with fields (a list headed by a symbol). It does **not**
/// apply to a transparent newtype, which by clause 2 emits no name at all.
#[track_caller]
fn projects_under_its_own_name<T: TextValue + std::fmt::Debug>(value: &T) {
    let debug = format!("{value:?}");
    let rust_name = debug
        .split(['(', ' ', '{'])
        .next()
        .expect("Debug output is non-empty");
    let expected = kebab(rust_name);

    let projected = value.project();
    let head = match &projected {
        Sexp::Symbol(name) => name.clone(),
        Sexp::List(items) => items
            .first()
            .and_then(Sexp::as_symbol)
            .unwrap_or_else(|| {
                panic!("{rust_name} projects to a list not headed by a symbol: {projected:?}")
            })
            .to_owned(),
        other => panic!("{rust_name} projects to {other:?}, which carries no constructor name"),
    };

    assert_eq!(
        head, expected,
        "{rust_name} projects under the name `{head}`, but its Rust name kebabs to \
         `{expected}`. A round-trip test cannot see this: `parse` reads the same \
         wrong symbol `project` wrote."
    );
}

mod event {
    use super::projects_under_its_own_name;
    use epiphany_core::{
        Event, EventDuration, EventPosition, GraceKind, IndeterminacyKind, MusicalDuration,
        MusicalPosition, PitchId, RationalTime, ReplicaId, Rest, StaffPosition, TrajectoryEndpoint,
        TrajectoryShape,
    };

    fn replica() -> ReplicaId {
        ReplicaId::from_entropy([1; 8]).expect("a non-system-derived replica")
    }

    fn a_rest() -> Rest {
        Rest {
            id: epiphany_core::EventId::new(replica(), 1),
            voice: epiphany_core::VoiceId::new(replica(), 1),
            position: EventPosition::Musical(MusicalPosition(RationalTime::zero())),
            duration: EventDuration::Musical(MusicalDuration(
                RationalTime::new(1, 4).expect("a valid quarter"),
            )),
            vertical_position: Some(StaffPosition(-2)),
            visible: true,
        }
    }

    #[test]
    fn grace_kind_variants_project_under_their_own_names() {
        for value in [
            GraceKind::Acciaccatura,
            GraceKind::Appoggiatura,
            GraceKind::Unmeasured,
        ] {
            projects_under_its_own_name(&value);
        }
    }

    #[test]
    fn indeterminacy_kind_variants_project_under_their_own_names() {
        for value in [
            IndeterminacyKind::Pitch,
            IndeterminacyKind::Duration,
            IndeterminacyKind::Choice,
        ] {
            projects_under_its_own_name(&value);
        }
        projects_under_its_own_name(&IndeterminacyKind::Compound(vec![IndeterminacyKind::Pitch]));
    }

    #[test]
    fn trajectory_variants_project_under_their_own_names() {
        projects_under_its_own_name(&TrajectoryShape::Linear);
        projects_under_its_own_name(&TrajectoryEndpoint::EventPitch(PitchId::new(replica(), 1)));
    }

    /// Also covers a `struct_codec!` struct (`Rest`), whose head comes from the
    /// macro, and the `Event` variant that wraps it, whose head is hand-spelled.
    #[test]
    fn event_variants_project_under_their_own_names() {
        let rest = a_rest();
        projects_under_its_own_name(&rest);
        projects_under_its_own_name(&Event::Rest(rest));
    }
}
