#[macro_use]
extern crate influx_db_client;
#[macro_use]
extern crate log;
extern crate rpassword;

use std::borrow::Borrow;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread::sleep;
use std::time::Duration;
use std::io::{stdin, stdout, Write};

use clap::{App, Arg, ArgMatches, SubCommand};
use dirs::home_dir;

use tesla::{TeslaClient, Vehicle, TeslaError};

use crate::config::{Config, GlobalConfig};
use crate::influx::run_influx_reporter;

mod config;
mod influx;
mod error;

#[tokio::main]
async fn main() {
    std::process::exit(match run().await {
        Ok(_) => 0,
        Err(_) => 1
    });
}

async fn run() -> Result<(), ()> {
    let matches = App::new("Tesla Control")
        .version("0.2.0")
        .author("Ze'ev Klapow <zklapow@gmail.com>")
        .about("A command line interface for your Tesla")
        .arg(
            Arg::with_name("debug-server")
                .short("d")
                .long("debug-server")
                .value_name("URL")
                .help("Provide a debug server (ex : http://localhost:4321/api/1/) to use instead of the official one from Tesla. Can be used to test/use the lib without having a valid Tesla account.")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file path")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("oauth")
                .short("o")
                .long("oauth")
                .help("Performs authentication with the Tesla servers using the prompted email address and password. Returns an oauth token when successful.")
                .takes_value(false)
        )
        .arg(
            Arg::with_name("vehicle")
                .long("vehicle")
                .short("V")
                .help("Name of vehicle to awaken")
                .global(true)
                .takes_value(true)
        )
        .subcommand(
            SubCommand::with_name("wake")
                .about("wake up the specified vehicle")
                .arg(
                    Arg::with_name("await")
                        .help("Wait for vehicle to awaken")
                        .long("await")
                        .short("a")
                        .takes_value(false)
                )
                .arg(
                    Arg::with_name("poll-interval")
                        .help("How quickly to poll the vehicle (in seconds)")
                        .long("poll-interval")
                        .short("p")
                        .takes_value(true)
                        .default_value("5")
                )
        )
        .subcommand(
            SubCommand::with_name("get_all_data")
                .about("get all the data for the specified vehicle")
        )
        .subcommand(
            SubCommand::with_name("flash_lights")
                .about("flash lights for the specified vehicle")
        )
        .subcommand(
            SubCommand::with_name("door_unlock")
                .about("unlock the doors for the specified vehicle")
        )
        .subcommand(
            SubCommand::with_name("door_lock")
                .about("lock the doors for the specified vehicle")
        )
        .subcommand(
            SubCommand::with_name("influx")
                .about("Start the influxdb reporter")
                .arg(
                    Arg::with_name("daemon")
                        .help("Daemonize the reporter process")
                        .long("daemon")
                        .short("d")
                        .takes_value(false)
                )
        )
        .get_matches();

    let debug_server = matches.value_of("debug-server");
    if debug_server.is_some() {
        println!("Using the debug server : {}", debug_server.unwrap());
    }

    if matches.is_present("oauth") {
        let token = auth_interactive(debug_server).await;
        return if token.is_ok() {
            println!("Your token is: {}", token.unwrap());
            Ok(())
        } else {
            println!("Token error: {}", token.err().unwrap());
            Err(())
        }
    }

    let config_path_default = home_dir()
        .unwrap_or(PathBuf::from("/"))
        .join(".teslac");

    let config_path = matches.value_of("config")
        .map(|p| PathBuf::from(p))
        .unwrap_or(config_path_default);
    let cfg = get_config(config_path.borrow(), debug_server.is_some());

    let mut config = match cfg {
        None => {
            // without config, go to auth progress
            let token = auth_interactive(debug_server).await;
            match token {
                Ok(t) => {
                    let new_config = Config {
                        global: GlobalConfig {
                            api_token: t,
                            default_vehicle: None,
                            default_vehicle_id: None,
                            logspec: Some("info".to_string())
                        },
                        influx: None
                    };

                    if let Ok(str_content) = toml::to_string(&new_config) {
                        fs::write(&config_path, str_content);
                    }

                    new_config
                }
                Err(_) => {
                    return Err(())
                }
            }
        }
        Some(c) => c
    };

    let client = if debug_server.is_some() {
        TeslaClient::new(debug_server.unwrap(), config.global.api_token.as_str())
    } else {
        TeslaClient::default(config.global.api_token.as_str())
    };

    flexi_logger::Logger::with_env_or_str(config.global.logspec.clone().unwrap_or("".to_owned()))
        .format(flexi_logger::colored_with_thread)
        .start()
        .unwrap();

    let vehicle_name = matches.value_of("vehicle")
        .map(|s| s.to_owned())
        .or(config.global.default_vehicle.clone());

    let vehicle_name = match vehicle_name {
        None => {
            let result = choose_vehicle(&mut config, &config_path, client.clone()).await;
            result.expect("fail to choose vehicle")
        }
        Some(n) => n
    };

    if let Some(submatches) = matches.subcommand_matches("wake") {
        cmd_wake(submatches, vehicle_name, client.clone()).await;
    } else if let Some(_submatches) = matches.subcommand_matches("get_all_data") {
        get_all_data(vehicle_name, client.clone()).await;
    } else if let Some(_submatches) = matches.subcommand_matches("flash_lights") {
        flash_lights(vehicle_name, client.clone()).await;
    } else if let Some(_submatches) = matches.subcommand_matches("door_unlock") {
        door_unlock(vehicle_name, client.clone()).await;
    } else if let Some(_submatches) = matches.subcommand_matches("door_lock") {
        door_lock(vehicle_name, client.clone()).await;
    } else if let Some(_submatches) = matches.subcommand_matches("influx") {
        if config.influx.is_none() {
            error!("No influx configuration present, cannot start influx reporter!");
            return Err(());
        }

        if let Err(e) = run_influx_reporter(config.influx.unwrap(), vehicle_name, client.clone()).await {
            error!("Error in influx reporter: {}", e);
            exit(1);
        }
    } else {
        println!("No command specified")
    }

    Ok(())
}

fn get_config(config_path: &Path, has_debug_server: bool) -> Option<Config> {
    if !config_path.exists() {
        return None;
    }

    // provide a default config if using the debug server
    let config_data = if has_debug_server {
        fs::read_to_string(config_path).unwrap_or_else(|_| -> String {
            let mut default_config_content :String = String::new();
            default_config_content.push_str("[global]\n");
            default_config_content.push_str("api_token = \"abcdefghijklmnop1234567890\"\n");
            default_config_content.push_str("logspec = \"info\"\n");
            default_config_content.push_str("default_vehicle = \"Test CAR\"\n");
            default_config_content
        })
    } else {
        fs::read_to_string(config_path).expect("Cannot read config")
    };
    let cfg: Config = toml::from_str(config_data.as_str()).expect("Cannot parse config");
    Some(cfg)
}

async fn cmd_wake(matches: &ArgMatches<'_>, name: String, client: TeslaClient) {
    if let Some(vehicle) = client.get_vehicle_by_name(name.as_str()).await.expect("Could not load vehicles") {
        let vclient = client.vehicle(vehicle.id);
        info!("Waking up");
        match vclient.wake_up().await {
            Ok(_) => info!("Sent wakeup command to {}", name),
            Err(e) => error!("Wake up failed {:?}", e)
        }

        if matches.is_present("await") {
            info!("Waiting for {} to wake up.", name);
            let sleep_dur_s = Duration::from_secs(
                matches.value_of("poll-interval").unwrap().parse::<u64>()
                    .expect("Could not parse poll interval")
            );

            loop {
                if let Some(vehicle) = vclient.get().await.ok() {
                    if vehicle.state == "online" {
                        break;
                    } else {
                        debug!("{} is not yet online (current state is {}), waiting.", name, vehicle.state);
                    }
                }

                sleep(sleep_dur_s);
            }
        }
    } else {
        error!("Could not find vehicle named {}", name);
    }
}

async fn get_all_data(name: String, client: TeslaClient) {
    if let Some(vehicle) = client.get_vehicle_by_name(name.as_str()).await.expect("Could not load vehicles") {
        dbg!(&vehicle);
        let vclient = client.vehicle(vehicle.id);
        info!("getting all data");
        match vclient.get_all_data().await {
            Ok(data) => info!("{:#?}", data),
            Err(e) => error!("get data failed {:?}", e)
        }
    } else {
        error!("Could not find vehicle named {}", name);
    }
}

async fn flash_lights(name: String, client: TeslaClient) {
    if let Some(vehicle) = client.get_vehicle_by_name(name.as_str()).await.expect("Could not load vehicles") {
        let vclient = client.vehicle(vehicle.id);
        info!("flashing lights");
        match vclient.flash_lights().await {
            Ok(_) => info!("Success"),
            Err(e) => error!("flashing lights failed {:?}", e)
        }
    } else {
        error!("Could not find vehicle named {}", name);
    }
}

async fn door_unlock(name: String, client: TeslaClient) {
    if let Some(vehicle) = client.get_vehicle_by_name(name.as_str()).await.expect("Could not load vehicles") {
        let vclient = client.vehicle(vehicle.id);
        info!("unlocking doors");
        match vclient.door_unlock().await {
            Ok(_) => info!("Success"),
            Err(e) => error!("unlocking doors failed {:?}", e)
        }
    } else {
        error!("Could not find vehicle named {}", name);
    }
}

async fn door_lock(name: String, client: TeslaClient) {
    if let Some(vehicle) = client.get_vehicle_by_name(name.as_str()).await.expect("Could not load vehicles") {
        let vclient = client.vehicle(vehicle.id);
        info!("locking doors");
        match vclient.door_lock().await {
            Ok(_) => info!("Success"),
            Err(e) => error!("locking doors failed {:?}", e)
        }
    } else {
        error!("Could not find vehicle named {}", name);
    }
}

async fn auth_interactive(debug_server: Option<&str>) -> Result<String, TeslaError> {
    let mut email = String::new();
    print!("Please enter your email: ");
    let _ = stdout().flush();
    stdin().read_line(&mut email).expect("Did not enter a correct string");
    email = email.replace("\n", "").replace("\r", "");

    let password = rpassword::prompt_password_stdout("Password: ").unwrap();
    let token = if debug_server.is_some() {
        TeslaClient::authenticate_using_api_root(debug_server.unwrap(), email.as_str(), password.as_str()).await
    } else {
        TeslaClient::authenticate(email.as_str(), password.as_str()).await
    };

    token
}

async fn choose_vehicle(config: &mut Config, config_path: &PathBuf, client: TeslaClient) -> Result<String, TeslaError> {
    println!("No default vehicle and no vehicle specified, please select:");
    let vehicles = client.get_vehicles().await;
    match vehicles {
        Ok(v_list) => {
            println!("index, name, state");
            for (i, v) in v_list.iter().enumerate() {
                println!("[{}], {}, {}", i + 1, v.display_name, v.state);
            }
            print!("Please enter index: ");
            let _ = stdout().flush();
            let mut index_to_input: String = String::new();
            stdin().read_line(&mut index_to_input).expect("Did not enter a correct index");
            index_to_input = index_to_input.replace("\n", "").replace("\r", "");

            let i: usize = index_to_input.parse().expect("Did not enter a correct index");
            if i > v_list.len() || i < 1 {
                return Err(TeslaError::SystemError);
            }

            config.global.default_vehicle_id = Some(v_list[i - 1].id);
            config.global.default_vehicle = Some(v_list[i - 1].display_name.clone());

            if let Ok(str_content) = toml::to_string(config) {
                fs::write(config_path, str_content);
            }

            Ok(v_list[i - 1].display_name.clone())
        }
        Err(e) => {
            error!("Fail to get vehicle list");
            Err(e)
        }
    }
}