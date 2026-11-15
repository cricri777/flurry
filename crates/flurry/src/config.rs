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
    resolve(&config_dir()?, name, &env::vars().collect::<Vec<_>>())
}

// Pure core of `load`: the environment is passed in so it can be tested.
fn resolve(
    dir: &std::path::Path,
    name: Option<&str>,
    vars: &[(String, String)],
) -> Result<Connection, String> {
    let var = |k: &str| vars.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    let config = read(dir.join("config.toml"))?;
    let name = name
        .map(String::from)
        .or_else(|| var("SNOWFLAKE_DEFAULT_CONNECTION_NAME"))
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
    for (k, v) in vars {
        if let Some(key) = k.strip_prefix(&prefix) {
            conn.insert(key.to_ascii_lowercase(), v.clone());
        }
    }
    for key in ["account", "user", "role", "warehouse", "private_key_file"] {
        if let Some(v) = var(&format!("SNOWFLAKE_{}", key.to_ascii_uppercase())) {
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

/// Base directory for flurry's cache files.
pub fn cache_dir() -> Option<PathBuf> {
    let base = env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .ok()?;
    Some(base.join("flurry"))
}

/// Base directory for flurry's database.
pub fn data_dir() -> Option<PathBuf> {
    let base = env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .ok()?;
    Some(base.join("flurry"))
}

/// Best-effort write of a 0600 file inside a 0700 directory.
pub fn write_private(path: &std::path::Path, bytes: &[u8]) {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let Some(dir) = path.parent() else { return };
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    if let Ok(mut f) = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
    {
        let _ = f.write_all(bytes);
    }
}

/// Best-effort chmod for local data.
pub fn restrict(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let d = env::temp_dir().join(format!("flurry-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        for (f, body) in files {
            fs::write(d.join(f), body).unwrap();
        }
        d
    }

    fn env_of(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    const CONFIG: &str = "default_connection_name = \"prod\"\n[connections.prod]\naccount = \"A\"\nuser = \"u\"\n[connections.dev]\naccount = \"B\"\n";

    #[test]
    fn uses_default_connection_name() {
        let c = resolve(&dir("default", &[("config.toml", CONFIG)]), None, &[]).unwrap();
        assert_eq!(c["account"], "A");
    }

    #[test]
    fn explicit_name_beats_env_and_file() {
        let d = dir("explicit", &[("config.toml", CONFIG)]);
        let vars = env_of(&[("SNOWFLAKE_DEFAULT_CONNECTION_NAME", "prod")]);
        assert_eq!(resolve(&d, Some("dev"), &vars).unwrap()["account"], "B");
    }

    #[test]
    fn env_default_name_beats_file() {
        let d = dir("envname", &[("config.toml", CONFIG)]);
        let vars = env_of(&[("SNOWFLAKE_DEFAULT_CONNECTION_NAME", "dev")]);
        assert_eq!(resolve(&d, None, &vars).unwrap()["account"], "B");
    }

    #[test]
    fn falls_back_to_connection_named_default() {
        let d = dir(
            "fallback",
            &[("config.toml", "[connections.default]\naccount = \"X\"\n")],
        );
        assert_eq!(resolve(&d, None, &[]).unwrap()["account"], "X");
    }

    #[test]
    fn connections_toml_replaces_config_connections() {
        let d = dir(
            "connections",
            &[
                ("config.toml", CONFIG),
                ("connections.toml", "[prod]\naccount = \"C\"\n"),
            ],
        );
        let c = resolve(&d, None, &[]).unwrap();
        assert_eq!(c["account"], "C");
        assert!(!c.contains_key("user"));
    }

    #[test]
    fn per_connection_env_overrides_and_generic_env_fills_gaps() {
        let d = dir("env", &[("config.toml", CONFIG)]);
        let vars = env_of(&[
            ("SNOWFLAKE_CONNECTIONS_PROD_PASSWORD", "secret"),
            ("SNOWFLAKE_CONNECTIONS_PROD_ACCOUNT", "Z"),
            ("SNOWFLAKE_ACCOUNT", "ignored"),
            ("SNOWFLAKE_ROLE", "R"),
        ]);
        let c = resolve(&d, None, &vars).unwrap();
        assert_eq!(c["password"], "secret");
        assert_eq!(c["account"], "Z");
        assert_eq!(c["role"], "R");
    }

    #[test]
    fn unknown_connection_is_an_error() {
        let d = dir("unknown", &[("config.toml", CONFIG)]);
        assert!(
            resolve(&d, Some("nope"), &[])
                .unwrap_err()
                .contains("'nope' not found")
        );
    }

    #[test]
    fn invalid_toml_is_an_error() {
        let d = dir("invalid", &[("config.toml", "[connections")]);
        assert!(resolve(&d, None, &[]).is_err());
    }

    #[test]
    fn expands_home_only_for_tilde_prefix() {
        let home = env::var("HOME").unwrap();
        assert_eq!(expand("~/k.p8"), PathBuf::from(home).join("k.p8"));
        assert_eq!(expand("/abs/k.p8"), PathBuf::from("/abs/k.p8"));
    }
}
