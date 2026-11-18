fn main() {
    // Nelle build di sviluppo (anche `cargo test`) il codice generato porta le informazioni che
    // servono ai test sull'accessibilità (ElementHandle del backend di prova); in release no.
    let sviluppo = std::env::var("PROFILE").is_ok_and(|p| p == "debug");
    let config = slint_build::CompilerConfiguration::new().with_debug_info(sviluppo);
    slint_build::compile_with_config("ui/cornice.slint", config).expect("compilazione della cornice Slint");
}
