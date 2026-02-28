use super::simulation_warning_message;

#[test]
fn simulation_warning_includes_mode_and_safety_note() {
    let message = simulation_warning_message(true, 8).expect("message");
    assert!(message.contains("Simulation mode active"));
    assert!(message.contains("Not for live hardware"));
    assert!(message.contains("x8"));
}

#[test]
fn simulation_warning_omitted_in_production_mode() {
    assert!(simulation_warning_message(false, 1).is_none());
}
