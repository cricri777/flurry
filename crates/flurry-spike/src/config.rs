use std::{collections::HashMap, env, fs, path::PathBuf};

pub type Connection = HashMap<String, String>;

// Mirrors Snowflake CLI: SNOWFLAKE_HOME, then ~/.snowflake, then ~/.config/snowflake.
fn config_dir() -> Result<PathBuf, String> {
    if let Ok(h) = env::var("SNOWFLAKE_HOME") {
        return Ok(h.into());
    }
    let home = PathBuf::from(env::var("HOME").map_err(|_| "HOME not set")?);
    let legacy = home.join(".snowflake");
    Ok(if legacy.exists() {
        legacy
    } else {
        home.join(".config/snowflake")
    })
}

fn read(path: PathBuf) -> Result<Option<toml::Table>, String> {
    match fs::read_to_string(&path) {
        Ok(s) => s.parse().map(Some).map_err(|e| format!("{path:?}: {e}")),
        Err(_) => Ok(None),
    }
}

/// Loads a connection; `name` overrides the default connection.
pub fn load(name: Option<&str>) -> Result<Connection, String> {
    let dir = config_dir()?;
    let config = read(dir.join("config.toml"))?;
    let name = name
        .map(String::from)
        .or_else(|| env::var("SNOWFLAKE_DEFAULT_CONNECTION_NAME").ok())
        .or_else(|| {
            config
                .as_ref()?
                .get("default_connection_name")?
                .as_str()
                .map(String::from)
        })
        .unwrap_or_else(|| "default".into());

    // connections.toml takes over entirely when present.
    let table = match read(dir.join("connections.toml"))? {
        Some(t) => t,
        None => config
            .and_then(|mut c| c.remove("connections"))
            .and_then(|c| c.try_into().ok())
            .unwrap_or_default(),
    };
    let mut conn: Connection = table
        .get(&name)
        .and_then(|v| v.as_table())
        .ok_or(format!("Connection '{name}' not found in {dir:?}"))?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();

    // SNOWFLAKE_CONNECTIONS_<NAME>_<KEY> overrides, then generic SNOWFLAKE_<KEY> fills gaps.
    let prefix = format!("SNOWFLAKE_CONNECTIONS_{}_", name.to_ascii_uppercase());
    for (k, v) in env::vars() {
        if let Some(key) = k.strip_prefix(&prefix) {
            conn.insert(key.to_ascii_lowercase(), v);
        }
    }
    for key in ["account", "user", "role", "warehouse", "private_key_file"] {
        if let Ok(v) = env::var(format!("SNOWFLAKE_{}", key.to_ascii_uppercase())) {
            conn.entry(key.into()).or_insert(v);
        }
    }
    Ok(conn)
}

/// Expands a leading `~/` in paths.
pub fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), env::var("HOME")) {
        (Some(rest), Ok(h)) => PathBuf::from(h).join(rest),
        _ => path.into(),
    }
}
