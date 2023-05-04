use config::{Config, ConfigError};
use core::time;
use std::net::UdpSocket;
use std::str::from_utf8;
use std::sync::Mutex;
use std::sync::mpsc::channel;
use std::thread;
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
use std::{process, thread::sleep, thread::spawn};

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

    let roller_pattern = "history/plclog_{}.gz";
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
        .build("plc.log", compound_policy)
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
    const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
    const LOG_PATTERN: &str = "{d(%Y-%m-%d %H:%M:%S%.6f)} | {({l}):5.5} | {m}{n}";
    const LOG_PATTERN_PLC: &str = "{m}{n}";
    
    // read config.toml file
    let app_config = app_config()
        .unwrap_or_else(|err| {
            println!("{err}");
            process::exit(1);
        });
    
    // setup logger
    let log_handle = logger_setup(&app_config, LOG_PATTERN);

    // start application
    info!("Rusty PLC Logger v{APP_VERSION} - Starting Up...");

    // setup channel to be used to communicate across threads
    let (tx, rx) = channel();

    // udp listener
    let listening_port = app_config.listening_port;

    // spawn a thread to handle the UDP socket
    let address_with_port = String::from("0.0.0.0:") + &listening_port.to_string();

    thread::spawn(move || {
        let socket = UdpSocket::bind(address_with_port)
                           .unwrap_or_else(|err| {
                                error!("{err}");
                                error!("Check if another instance of the logger is running, or if another application is using port {}", &listening_port);
                                process::exit(1);
                           });
    info!("Starting UDP Listener on port: {}", &listening_port);

    loop {
        let tx_thread = tx.clone();
        let mut buf = [0u8; 1500];
        info!("Cloning socket...");
        let sock = socket.try_clone()
            .unwrap_or_else(|err| {
                    error!("{err}");
                    process::exit(1);
                });

        info!("Waiting for packet...");
        match sock.recv_from(&mut buf) {
            Ok((amt, src)) => {
                thread::spawn(move || {
                    info!("Handling connection from {}", src);
                    let buf = &mut buf[..amt];
                    let string_data = from_utf8(buf).unwrap().to_string();
                    tx_thread.send(string_data)
                        .unwrap_or_else(|err| {
                            error!("{err}");
                        });
                
                });
            },
            Err(e) => {
                error!("{}", e);
            }
        }
    }
    });

    for r in rx {
        let log_handle = logger_setup(&app_config, LOG_PATTERN_PLC);
        info!("{r}");
    }

    /* for _ in 0..5 {
        sleep(time::Duration::from_millis(1000));
        error!("first log error");
        info!("first log info");
        log_handle.set_config(logger_config(LOG_PATTERN_PLC, &app_config));
    } */

    log_handle.set_config(logger_config(LOG_PATTERN, &app_config));
    info!("last log");
}
