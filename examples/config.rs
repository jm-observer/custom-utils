#[tokio::main]
async fn main() {
    println!("{}", custom_utils::args::expand_path(".//example").unwrap().display());
    println!("{}", custom_utils::args::expand_path("~//example").unwrap().display());
    println!("{}", custom_utils::args::get_user_home().unwrap().display());
    println!(
        "{}",
        custom_utils::args::workspace(&None::<String>, "desk")
            .unwrap()
            .display()
    );
}
