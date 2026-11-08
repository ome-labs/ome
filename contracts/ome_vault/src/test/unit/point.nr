//! `Point` predicates and `assert_ordered_points`, the shared rule for signer keys and committee shares.

use crate::{constants::MAX_SIGNERS, point::{assert_ordered_points, Point}};
use crate::test::fixtures::points::{distinct_points, off_curve_point, point_from_scalar};
use std::embedded_curve_ops::EmbeddedCurvePoint;

#[test]
fn empty_point_is_empty() {
    assert(Point::empty().is_empty());
}

#[test]
fn point_with_one_zero_coordinate_is_not_empty() {
    assert(!Point { x: 0, y: 1 }.is_empty());
    assert(!Point { x: 1, y: 0 }.is_empty());
}

#[test]
fn empty_point_is_not_on_curve() {
    assert(!Point::empty().is_on_curve());
}

#[test]
fn generator_and_its_negation_are_on_curve() {
    let g = EmbeddedCurvePoint::generator();

    assert(Point { x: g.x, y: g.y }.is_on_curve());
    assert(Point { x: g.x, y: -g.y }.is_on_curve());
}

#[test]
fn multiples_of_the_generator_are_on_curve() {
    for scalar in 1..10 {
        assert(point_from_scalar(scalar as Field).is_on_curve());
    }
}

#[test]
fn arbitrary_point_is_not_on_curve() {
    assert(!off_curve_point().is_on_curve());
}

#[test]
fn to_embedded_keeps_coordinates() {
    let point = point_from_scalar(7);
    let embedded = point.to_embedded();

    assert_eq(embedded.x, point.x);
    assert_eq(embedded.y, point.y);
}

#[test]
fn accepts_no_points() {
    assert_ordered_points([Point::empty(); MAX_SIGNERS], 0, "signer");
}

#[test]
fn accepts_a_single_point() {
    assert_ordered_points(distinct_points(1, 1), 1, "signer");
}

#[test]
fn accepts_a_full_array() {
    assert_ordered_points(distinct_points(MAX_SIGNERS, 1), MAX_SIGNERS, "signer");
}

#[test(should_fail_with = "empty signer key")]
fn rejects_an_empty_point_inside_count() {
    let mut points = distinct_points(3, 1);
    points[1] = Point::empty();

    assert_ordered_points(points, 3, "signer");
}

#[test(should_fail_with = "signer key not on curve")]
fn rejects_an_off_curve_point() {
    let mut points = distinct_points(3, 1);
    points[2] = off_curve_point();

    assert_ordered_points(points, 3, "signer");
}

#[test(should_fail_with = "duplicate signer")]
fn rejects_adjacent_duplicates() {
    let mut points = distinct_points(3, 1);
    points[1] = points[0];

    assert_ordered_points(points, 3, "signer");
}

#[test(should_fail_with = "duplicate signer")]
fn rejects_distant_duplicates() {
    let mut points = distinct_points(MAX_SIGNERS, 1);
    points[MAX_SIGNERS - 1] = points[0];

    assert_ordered_points(points, MAX_SIGNERS, "signer");
}

#[test(should_fail_with = "duplicate signer")]
fn rejects_a_point_and_its_copy_at_any_distance() {
    let mut points = distinct_points(5, 1);
    points[4] = points[2];

    assert_ordered_points(points, 5, "signer");
}

#[test(should_fail_with = "non-empty signer slot past count")]
fn rejects_a_point_right_after_count() {
    assert_ordered_points(distinct_points(3, 1), 2, "signer");
}

#[test(should_fail_with = "non-empty signer slot past count")]
fn rejects_a_point_in_the_last_slot_past_count() {
    let mut points = distinct_points(2, 1);
    points[MAX_SIGNERS - 1] = point_from_scalar(42);

    assert_ordered_points(points, 2, "signer");
}

#[test(should_fail_with = "non-empty signer slot past count")]
fn rejects_any_point_when_count_is_zero() {
    assert_ordered_points(distinct_points(1, 1), 0, "signer");
}

#[test(should_fail_with = "committee key not on curve")]
fn prefixes_failures_with_the_label() {
    let mut points = distinct_points(2, 1);
    points[0] = off_curve_point();

    assert_ordered_points(points, 2, "committee");
}
