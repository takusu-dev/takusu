use std::borrow::Cow;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Initialize Sentry and the global `tracing` subscriber.
///
/// Reads the DSN from the `SENTRY_DSN` environment variable. If it is not set
/// or invalid, Sentry is disabled and only the local `tracing` output is
/// configured.
///
/// `default_filter` is added on top of `RUST_LOG` so that build-time defaults
/// (e.g. `takusu_local=info`) are preserved while still allowing `RUST_LOG` to
/// override them.
///
/// `release` should be the binary's release name (e.g. `takusu-local@x.y.z`).
/// If `None`, Sentry's release field is left empty.
pub fn init(
    default_filter: &str,
    release: Option<Cow<'static, str>>,
) -> sentry::ClientInitGuard {
    let dsn: Option<sentry::types::Dsn> = std::env::var("SENTRY_DSN").ok().and_then(|s| {
        s.parse::<sentry::types::Dsn>()
            .inspect_err(|e| eprintln!("ignoring invalid SENTRY_DSN: {e}"))
            .ok()
    });

    let guard = sentry::init(sentry::ClientOptions {
        dsn,
        release,
        traces_sample_rate: 1.0,
        ..Default::default()
    });

    let mut env_filter = EnvFilter::from_default_env();
    if let Ok(directive) = default_filter.parse() {
        env_filter = env_filter.add_directive(directive);
    }

    let fmt_filter = env_filter.clone();
    let sentry_filter = env_filter;

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(fmt_filter))
        .with(sentry::integrations::tracing::layer().with_filter(sentry_filter))
        .init();

    guard
}
