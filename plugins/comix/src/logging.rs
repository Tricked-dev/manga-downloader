use std::sync::Once;

pub fn log_plugin_loading() {
    static LOADED: Once = Once::new();

    LOADED.call_once(|| {
        tracing::info!(
            plugin = "comix",
            version = env!("CARGO_PKG_VERSION"),
            "Plugin Loaded",
        );
    });
}
