use super::*;

#[test]
fn describes_call_flags() {
    assert_eq!(describe_call_flags(0x00), "None");
    assert_eq!(describe_call_flags(CALL_FLAG_READ_STATES), "ReadStates");
    assert_eq!(
        describe_call_flags(CALL_FLAG_READ_STATES | CALL_FLAG_ALLOW_CALL),
        "ReadStates|AllowCall"
    );
    assert_eq!(
        describe_call_flags(CALL_FLAGS_ALLOWED_MASK),
        "ReadStates|WriteStates|AllowCall|AllowNotify"
    );
}

#[test]
fn call_flag_labels_report_individual_bits() {
    let labels = call_flag_labels(CALL_FLAG_READ_STATES | CALL_FLAG_ALLOW_NOTIFY);
    assert_eq!(labels, vec!["ReadStates", "AllowNotify"]);
}
