use log::debug;
use custom_utils::parse_file;

#[tokio::main]
async fn main() {

    custom_utils::logger::logger_stdout_debug();
    let codes = r#"
enum MergeEvent {
    AEvent(AEvent),
    Close(Close),
}"#;
    debug!("{:?}", parse_file(codes).await.unwrap());

    let codes = r#"
enum MergeEvent {
    AEvent {code: u16},
    Close(Close),
}"#;
    debug!("{:?}", parse_file(codes).await.unwrap());


}