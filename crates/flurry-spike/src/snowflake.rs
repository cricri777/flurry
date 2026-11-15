use crate::{config, sso};
use snowflake_api::SnowflakeApi;

// Read-only guard: only SHOW and SELECT are ever executed.
fn guard(sql: &str) -> Result<(), String> {
    let first = sql
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    if first == "SHOW" || first == "SELECT" {
        Ok(())
    } else {
        Err("Read-only: statement rejected".into())
    }
}

enum Backend {
    Key(SnowflakeApi),
    Sso(sso::Session),
}

impl Backend {
    async fn exec(&mut self, sql: &str) -> Result<usize, String> {
        match self {
            Backend::Key(api) => {
                let v = api.exec_json(sql).await.map_err(|e| e.to_string())?;
                Ok(v.as_array().map_or(0, |a| a.len()))
            }
            Backend::Sso(s) => s.exec(sql).await,
        }
    }

    async fn close(&mut self) -> Result<(), String> {
        match self {
            Backend::Key(api) => api.close_session().await.map_err(|e| e.to_string()),
            Backend::Sso(_) => Ok(()),
        }
    }
}

pub async fn run(name: Option<&str>) -> Result<(), String> {
    let c = config::load(name)?;
    let get = |k: &str| c.get(k).map(String::as_str);
    let auth = get("authenticator").unwrap_or("").to_ascii_lowercase();
    let mut backend = if auth == "externalbrowser" {
        Backend::Sso(sso::Session::connect(&c, false).await?)
    } else if auth == "username_password_mfa" {
        Backend::Sso(sso::Session::connect(&c, true).await?)
    } else {
        let need = |k: &str| get(k).ok_or(format!("Missing '{k}' in connection"));
        let key_path = get("private_key_file")
            .or_else(|| get("private_key_path"))
            .ok_or("Missing 'private_key_file' in connection")?;
        let key = std::fs::read_to_string(config::expand(key_path)).map_err(|e| e.to_string())?;
        Backend::Key(
            SnowflakeApi::with_certificate_auth(
                need("account")?,
                get("warehouse"),
                None,
                None,
                need("user")?,
                get("role"),
                &key,
            )
            .map_err(|e| e.to_string())?,
        )
    };

    for sql in [
        "SHOW ROLES",
        "SELECT COUNT(*) AS N FROM SNOWFLAKE.ACCOUNT_USAGE.GRANTS_TO_USERS WHERE DELETED_ON IS NULL",
    ] {
        guard(sql)?;
        println!("{sql}\n  rows: {}", backend.exec(sql).await?);
    }
    backend.close().await
}
