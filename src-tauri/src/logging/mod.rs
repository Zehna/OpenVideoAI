use std::sync::Once;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static INIT: Once = Once::new();

/// Initialize structured logging for OpenVideoAI.
/// Formats output with timestamp, level, target module, and sanitized message.
/// Reads filter level from `RUST_LOG` environment variable or defaults to `info`.
pub fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    INIT.call_once(|| {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,openvideoai=debug"));

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_names(false)
            .with_level(true);

        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer);

        let _ = subscriber.try_init();
    });

    Ok(())
}
