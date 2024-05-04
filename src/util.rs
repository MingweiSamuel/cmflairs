//! Helper utilities.

use riven::RiotApi;
use web_sys::console;
use worker::{console_error, console_log, Env};

/// Initialize [`log`] logging into Cloudflare's [`console`] logging system, if not already
/// initialized.
pub fn init_logging() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        {
            fn hook(info: &std::panic::PanicInfo) {
                console_error!("{}", info);
            }
            std::panic::set_hook(Box::new(hook));
            console_log!("[panic hook set]");
        }
        {
            struct ConsoleLog;
            static LOG: ConsoleLog = ConsoleLog;
            impl log::Log for ConsoleLog {
                fn enabled(&self, _metadata: &log::Metadata) -> bool {
                    true // TODO
                }

                fn log(&self, record: &log::Record) {
                    let method = match record.level() {
                        log::Level::Error => console::error_1,
                        log::Level::Warn => console::warn_1,
                        log::Level::Info => console::info_1,
                        log::Level::Debug => console::debug_1,
                        log::Level::Trace => console::trace_1,
                    };
                    (method)(
                        &format!(
                            "[{} {}] {}",
                            record.level(),
                            record.module_path().unwrap_or("?"),
                            record.args()
                        )
                        .into(),
                    );
                }

                fn flush(&self) {}
            }
            log::set_logger(&LOG).unwrap();
            log::set_max_level(log::LevelFilter::Trace); // TODO

            log::info!("logger set");
        }
    });
}

/// Initialize and return the [`RiotApi`] instance, if not already initialized.
pub fn get_rgapi(env: &Env) -> &'static RiotApi {
    use std::sync::OnceLock;
    static ONCE: OnceLock<RiotApi> = OnceLock::new();
    ONCE.get_or_init(|| {
        let rgapi = RiotApi::new(env.secret("RGAPI_KEY").unwrap().to_string());
        log::info!("rgapi initialized");
        rgapi
    })
}
