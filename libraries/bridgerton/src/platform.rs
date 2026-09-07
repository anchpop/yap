//! Platform utilities for application code shared by browser and native hosts:
//! time, delays, blocking work, logging, and process environment.
//!
//! Native async helpers use the Tokio runtime supplied by the host.

/// The current local offset from UTC, using the browser or native system timezone.
pub fn current_local_offset() -> chrono::FixedOffset {
    #[cfg(target_arch = "wasm32")]
    let offset_seconds = {
        // JavaScript reports minutes west of UTC; Chrono uses seconds east.
        let offset_minutes = js_sys::Date::new_0().get_timezone_offset();
        (-offset_minutes * 60.0) as i32
    };
    #[cfg(not(target_arch = "wasm32"))]
    let offset_seconds = chrono::Local::now().offset().local_minus_utc();

    chrono::FixedOffset::east_opt(offset_seconds)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).expect("UTC offset is always valid"))
}

/// Log elapsed milliseconds when this scope guard is dropped.
pub struct PerfTimer {
    label: String,
    start_time: web_time::Instant,
}

impl PerfTimer {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            start_time: web_time::Instant::now(),
        }
    }
}

impl Drop for PerfTimer {
    fn drop(&mut self) {
        log::info!(
            "[PERF] {}: {:.2}ms",
            self.label,
            self.start_time.elapsed().as_secs_f64() * 1000.0
        );
    }
}

/// Wait without blocking the browser event loop or native executor.
pub async fn sleep_ms(milliseconds: u32) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(milliseconds).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(milliseconds.into())).await;
}

/// Run synchronous work on Tokio's blocking pool natively, or inline in the browser.
///
/// The result is a join result; a fallible operation keeps its own inner `Result`.
/// Dropping the future does not stop work already running on the blocking pool.
pub async fn run_blocking<F, T>(work: F) -> std::io::Result<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        Ok(work())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::task::spawn_blocking(work)
            .await
            .map_err(std::io::Error::other)
    }
}

/// Initialize browser-console or native `RUST_LOG` logging once per process/module.
///
/// Native hosts that already installed a logger keep theirs. The optional
/// `console_error_panic_hook` feature also reports browser panics to the console.
pub fn init_logging() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        #[cfg(all(target_arch = "wasm32", feature = "console_error_panic_hook"))]
        console_error_panic_hook::set_once();
        #[cfg(target_arch = "wasm32")]
        wasm_logger::init(wasm_logger::Config::default());
        #[cfg(not(target_arch = "wasm32"))]
        let _ = env_logger::try_init();
        log::info!("Logging initialized");
    });
}

/// Read a native process environment variable; browsers have no process environment.
///
/// Missing or non-Unicode variables return `None`. Defaults and caching belong
/// to the caller, as do compile-time settings obtained with `option_env!`.
pub fn runtime_env(name: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = name;
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var(name).ok()
    }
}

/// Post a message to the other instances of this application on the same
/// device. Browser tabs share a `BroadcastChannel`; a native process is its
/// own only instance, so nothing is sent.
pub fn broadcast(channel: &str, message: &impl serde::Serialize) -> Result<(), crate::Error> {
    #[cfg(target_arch = "wasm32")]
    {
        let channel = web_sys::BroadcastChannel::new(channel)?;
        let message = serde_wasm_bindgen::to_value(message)
            .map_err(|error| crate::Error::new(error.to_string()))?;
        channel.post_message(&message)?;
        channel.close();
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (channel, message);
        Ok(())
    }
}
