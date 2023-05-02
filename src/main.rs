use config::{Config, ConfigError};
use core::time;
use log::LevelFilter;
use log::{error, info};
use log4rs::append::console::ConsoleAppender;
use log4rs::append::rolling_file::policy::compound::CompoundPolicy;
use log4rs::append::rolling_file::policy::compound::{
    roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger,
};
use log4rs::append::rolling_file::RollingFileAppender;
use log4rs::config::{Appender, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::Config as LogConfig;
use log4rs::{self, Handle};
use std::{process, thread::sleep};

struct AppConfig {
    listening_port: u16,
    log_max_size_mb: u128,
    log_history_to_keep: u32,
}

fn app_config() -> Result<AppConfig, ConfigError> {
    // read config.toml file
    let cfg;
    match Config::builder()
        .add_source(config::File::with_name("config.toml"))
        .build()
    {
        Ok(cfg_ok) => cfg = cfg_ok,
        Err(err) => return Err(err),
    };

    // check for keys in config.toml file
    let listening_port = match cfg.get_int("listening_port") {
        Ok(val_ok) => val_ok,
        Err(err) => return Err(err),
    };

    let log_max_size_mb = match cfg.get_int("log_max_size_mb") {
        Ok(val_ok) => val_ok,
        Err(err) => return Err(err),
    };

    let log_history_to_keep = match cfg.get_int("log_history_to_keep") {
        Ok(val_ok) => val_ok,
        Err(err) => return Err(err),
    };

    // check if values from config.toml file are in valid range
    if (listening_port < 0) || (listening_port > 65535) {
        return Err(ConfigError::Message(String::from(
            "listening port must be between 0 - 65535",
        )));
    }
    let listening_port: u16 = listening_port.try_into().unwrap();

    if (log_max_size_mb < 1) || (log_max_size_mb > 100) {
        return Err(ConfigError::Message(String::from(
            "max log size must be between 1 - 100 (mb)",
        )));
    }
    let log_max_size_mb: u128 = log_max_size_mb.try_into().unwrap();

    if (log_history_to_keep < 0) || (log_history_to_keep > 1000) {
        return Err(ConfigError::Message(String::from(
            "log history must be between 0 - 1000",
        )));
    }
    let log_history_to_keep: u32 = log_history_to_keep.try_into().unwrap();

    Ok(AppConfig {
        listening_port: listening_port,
        log_max_size_mb: log_max_size_mb,
        log_history_to_keep: log_history_to_keep,
    })
}

fn logger_setup(appconfig: &AppConfig, log_pattern: &str) -> Handle {
    let config = logger_config(log_pattern, appconfig);

    let handle = log4rs::init_config(config).unwrap();
    handle
}

fn logger_config(log_pattern: &str, appconfig: &AppConfig) -> LogConfig {
    let log_line_pattern = log_pattern;

    let trigger_size = byte_unit::n_mb_bytes!(appconfig.log_max_size_mb) as u64;
    let trigger = Box::new(SizeTrigger::new(trigger_size));

    let roller_pattern = "logs/history/plclog_{}.gz";
    let roller_count = appconfig.log_history_to_keep;
    let roller_base = 1;
    let roller = Box::new(
        FixedWindowRoller::builder()
            .base(roller_base)
            .build(roller_pattern, roller_count)
            .unwrap(),
    );

    let compound_policy = Box::new(CompoundPolicy::new(trigger, roller));

    let step_ap = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(log_line_pattern)))
        .build("logs/plc.log", compound_policy)
        .unwrap();

    let stdout = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(log_line_pattern)))
        .build();

    let appenders = vec![String::from("stdout"), String::from("step_ap")];

    let config = LogConfig::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .appender(Appender::builder().build("step_ap", Box::new(step_ap)))
        .build(
            Root::builder()
                .appenders(appenders)
                .build(LevelFilter::Debug),
        )
        .unwrap();
    config
}

fn main() {
    // constants
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const LOG_PATTERN: &str = "{d(%Y-%m-%d %H:%M:%S)} | {({l}):5.5} | {f}:{L} — {m}{n}";
    const LOG_PATTERN_PLC: &str = "{m}{n}";

    let app_config = app_config().unwrap_or_else(|err| {
        println!("{err}");
        process::exit(1);
    });

    let log_handle = logger_setup(&app_config, LOG_PATTERN);


    for _ in 0..5 {
        sleep(time::Duration::from_millis(1000));
        info!("{VERSION}");
        error!("first log error");
        info!("first log info");
        log_handle.set_config(logger_config(LOG_PATTERN_PLC, &app_config));
    }

    log_handle.set_config(logger_config(LOG_PATTERN, &app_config));
    info!("last log");
}
