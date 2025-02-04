// Unless explicitly stated otherwise all files in this repository are licensed
// under the MIT/Apache-2.0 License, at your convenience
//
// This product includes software developed at Datadog (https://www.datadoghq.com/). Copyright 2021 Datadog, Inc.

use serde_json::{from_str, Value};
use std::convert::{TryFrom, TryInto};
use std::{collections::HashMap, env, fs::read_to_string};

pub(crate) struct Config {
    pub(crate) setup: Option<Vec<String>>,
    pub(crate) run: Vec<String>,
    pub(crate) timeout: Option<u64>,
    pub(crate) env: HashMap<String, String>,
}

struct ProtoConfig {
    setup: Option<Vec<String>>,
    run: Option<Vec<String>>,
    timeout: Option<u64>,
    env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigError(String);

impl TryFrom<ProtoConfig> for Config {
    type Error = ConfigError;

    fn try_from(config: ProtoConfig) -> Result<Config, ConfigError> {
        Ok(Config {
            setup: config.setup,
            run: match config.run {
                Some(run) => run,
                None => return Err("'run' must be provided".into()),
            },
            timeout: config.timeout,
            env: config.env,
        })
    }
}

impl From<String> for ConfigError {
    fn from(string: String) -> Self {
        ConfigError(string)
    }
}

impl From<&str> for ConfigError {
    fn from(string: &str) -> Self {
        string.to_owned().into()
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        format!("{}", err).into()
    }
}

impl From<std::env::VarError> for ConfigError {
    fn from(err: std::env::VarError) -> Self {
        format!("{}", err).into()
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(err: serde_json::Error) -> Self {
        format!("{}", err).into()
    }
}

macro_rules! errify {
    ($format:expr, $val:expr) => {
        return Err(format!($format, $val).into())
    };
}

fn get_shell_command(
    obj: &serde_json::Map<String, Value>,
    name: &str,
) -> Result<Vec<String>, ConfigError> {
    let run = obj
        .get(name)
        .unwrap()
        .as_str()
        .ok_or(format!("'{}' must be a string", name))?;

    shlex::split(run)
        .ok_or_else(|| format!("'{}' must be a properly formed shell command", name).into())
}

fn get_env(env: &mut HashMap<String, String>, config_env: &Value) -> Result<(), ConfigError> {
    let config_env = config_env.as_object().ok_or("env must be an object")?;
    for (name, value) in config_env.iter() {
        let value = value.as_str().ok_or("env vars must be strings")?;
        env.insert(name.clone(), value.to_owned());
    }
    Ok(())
}

fn apply_config(config: &mut ProtoConfig, config_val: &Value) -> Result<(), ConfigError> {
    let config_val = config_val.as_object().ok_or("invalid json")?;

    if config_val.contains_key("run") {
        config.run = Some(get_shell_command(&config_val, "run")?);
    }

    if config_val.contains_key("setup") {
        config.setup = Some(get_shell_command(&config_val, "setup")?);
    }

    if let Some(timeout_val) = config_val.get("timeout") {
        config.timeout = Some(
            timeout_val
                .as_u64()
                .ok_or("'timeout' must be a positive integer")?,
        );
    }

    if let Some(env) = config_val.get("env") {
        get_env(&mut config.env, &env)?;
    }
    Ok(())
}

pub(crate) fn get_config(filename: String) -> Result<Config, ConfigError> {
    let mut config = ProtoConfig {
        setup: None,
        run: None,
        timeout: None,
        env: HashMap::new(),
    };
    let json_str = read_to_string(filename)?;
    let config_val: Value = from_str(&json_str)?;

    apply_config(&mut config, &config_val)?;

    if let Some(variants) = config_val.get("variants") {
        let variant_key = env::var("SIRUN_VARIANT")?;
        let config_json;
        if let Some(variants) = variants.as_array() {
            let variant_key: usize = variant_key.parse().unwrap();
            if variants.len() <= variant_key {
                errify!("variant index {} does not exist in array", variant_key);
            }
            config_json = Some(&variants[variant_key]);
        } else if let Some(variants) = variants.as_object() {
            config_json = match variants.get(&variant_key) {
                Some(val) => Some(val),
                None => errify!("variant key {} does not exist in object", variant_key),
            };
        } else {
            return Err("variants must be array or object".into());
        }
        apply_config(&mut config, &config_json.unwrap())?;
    }

    Ok(config.try_into()?)
}
