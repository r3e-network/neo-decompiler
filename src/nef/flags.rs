/// Return the individual call flag labels set on the provided mask.
pub fn call_flag_labels(flags: u8) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags & super::CALL_FLAG_READ_STATES != 0 {
        labels.push("ReadStates");
    }
    if flags & super::CALL_FLAG_WRITE_STATES != 0 {
        labels.push("WriteStates");
    }
    if flags & super::CALL_FLAG_ALLOW_CALL != 0 {
        labels.push("AllowCall");
    }
    if flags & super::CALL_FLAG_ALLOW_NOTIFY != 0 {
        labels.push("AllowNotify");
    }
    labels
}

/// Return a human-readable list of call flag names for displaying method tokens.
pub fn describe_call_flags(flags: u8) -> String {
    if flags == 0 {
        return "None".into();
    }
    call_flag_labels(flags).join("|")
}
