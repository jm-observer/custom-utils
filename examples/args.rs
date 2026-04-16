use custom_utils::args::{arg_value, command, exist_arg};

/// dns -c ./config/config.yaml -l
/// dns --config ./config/config.yaml --loop
fn main() {
    assert_eq!(arg_value("--config", "-c"), Some("./config/config.yaml".to_string()));
    assert!(exist_arg("--loop", "-l"));
    assert_eq!(command(), Some("dns".to_string()))
}
