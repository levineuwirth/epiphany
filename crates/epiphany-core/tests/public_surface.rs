//! P13-S29 pin 1b: the root re-export has an observer.
//!
//! An integration test, not a unit test: a unit test inside the crate reaches
//! `invariants::` regardless of what the root re-exports, so only a consumer
//! outside the crate can observe touch row 2 at all.
//!
//! **Type-level only.** It calls nothing. A call whose result were asserted
//! would put this file in M3's and M9's radii; the file stays type-level so the
//! question never arises.

use epiphany_core::{check_requirement, ViolationKind, WellFormednessViolation};

#[test]
fn public_violation_surface_is_reexported() {
    let _: fn(&epiphany_core::Score, &str) -> Vec<WellFormednessViolation> = check_requirement;
    let _ = |k: &ViolationKind| matches!(k, ViolationKind::Requirement(_));
}
