fn main() {
    println!(
        "{} {}",
        veoveo_service_gateway::component(),
        env!("CARGO_PKG_VERSION")
    );
}
