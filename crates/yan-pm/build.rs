fn main() {
    // Expose TARGET triple to the binary via env!("TARGET")
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );
}
