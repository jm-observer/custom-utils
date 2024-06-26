/// arg_value("--check", "-c")
pub fn arg_value(long: &str, short: &str) -> Option<String> {
    assert!(long.starts_with("--"));
    assert!(short.starts_with('-'));
    let mut is_val = false;
    for arg in std::env::args() {
        if is_val {
            return Some(arg);
        }
        is_val = arg == long || arg == short;
    }
    None
}

/// exist_arg("--check", "-c")
pub fn exist_arg(long: &str, short: &str) -> bool {
    assert!(long.starts_with("--"));
    assert!(short.starts_with('-'));
    for arg in std::env::args() {
        if arg == long || arg == short {
            return true;
        }
    }
    false
}
pub fn command() -> Option<String> {
    for (index, arg) in std::env::args().enumerate() {
        if index == 1 {
            return Some(arg);
        }
    }
    None
}
