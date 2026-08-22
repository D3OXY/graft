#[test]
fn v1_journey_is_reserved_for_black_box_acceptance() {
    // The real journey is added once the CLI and Hub have their public seams.
    assert_eq!(env!("CARGO_PKG_NAME"), "graft-e2e");
}
