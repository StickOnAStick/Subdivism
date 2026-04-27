fn main() {
    let bind_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:4000".to_string());
    subdivism::server::run_dedicated_udp(&bind_addr);
}
