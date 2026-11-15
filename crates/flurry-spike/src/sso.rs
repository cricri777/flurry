use crate::config::Connection;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use uuid::Uuid;

const APP: (&str, &str) = ("flurry", "0.1.0");

pub struct Session {
    http: reqwest::Client,
    base: String,
    token: String,
}

fn rid() -> String {
    Uuid::new_v4().to_string()
}

fn check(v: &Value) -> Result<&Value, String> {
    if v["success"].as_bool() == Some(true) {
        Ok(&v["data"])
    } else {
        Err(v["message"]
            .as_str()
            .unwrap_or("Snowflake request failed")
            .into())
    }
}

// Waits for the IdP redirect to localhost and extracts the `token` query parameter.
fn wait_for_token(listener: TcpListener) -> Result<String, String> {
    let (mut s, _) = listener.accept().map_err(|e| e.to_string())?;
    let mut buf = [0u8; 8192];
    let n = s.read(&mut buf).map_err(|e| e.to_string())?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let _ = s.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\nLogin complete, you can close this tab.",
    );
    req.split("token=")
        .nth(1)
        .and_then(|t| t.split([' ', '&']).next())
        .map(String::from)
        .ok_or("No token in browser callback".into())
}

// Returns (account name, user, base URL) for a connection.
fn endpoint(c: &Connection) -> Result<(String, String, String), String> {
    let account = c.get("account").ok_or("Missing 'account' in connection")?;
    let user = c.get("user").ok_or("Missing 'user' in connection")?;
    let name = account.split('.').next().unwrap_or(account).to_string();
    let base = format!(
        "https://{}.snowflakecomputing.com",
        account.to_ascii_lowercase()
    );
    Ok((name, user.clone(), base))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// One cache file per account/user/role/warehouse/database/schema combination.
fn cache_path(c: &Connection) -> Option<std::path::PathBuf> {
    let key: String = ["account", "user", "role", "warehouse", "database", "schema"]
        .iter()
        .map(|k| c.get(*k).map_or("", String::as_str))
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let dir = std::env::var("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")))
        .ok()?;
    Some(dir.join("flurry").join(format!("session-{key}.json")))
}

fn load_cache(c: &Connection) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(cache_path(c)?).ok()?).ok()?;
    (v["expires"].as_u64()? > now() + 60).then(|| v["token"].as_str().map(String::from))?
}

// Best effort: a cache failure must never break the run.
fn save_cache(c: &Connection, token: &str, expires: u64) {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let Some(path) = cache_path(c) else { return };
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
    {
        let _ = f.write_all(
            json!({"token": token, "expires": expires})
                .to_string()
                .as_bytes(),
        );
    }
}

impl Session {
    pub async fn login(c: &Connection) -> Result<Self, String> {
        let get = |k: &str| c.get(k).map(String::as_str);
        let account = get("account").ok_or("Missing 'account' in connection")?;
        let user = get("user").ok_or("Missing 'user' in connection")?;
        let base = format!(
            "https://{}.snowflakecomputing.com",
            account.to_ascii_lowercase()
        );
        let name = account.split('.').next().unwrap_or(account);
        let http = reqwest::Client::new();

        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let auth: Value = http
            .post(format!(
                "{base}/session/authenticator-request?request_id={}",
                rid()
            ))
            .json(&json!({"data": {
                "ACCOUNT_NAME": name, "LOGIN_NAME": user,
                "AUTHENTICATOR": "EXTERNALBROWSER", "BROWSER_MODE_REDIRECT_PORT": port.to_string(),
                "CLIENT_APP_ID": APP.0, "CLIENT_APP_VERSION": APP.1,
            }}))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let data = check(&auth)?;
        let sso_url = data["ssoUrl"].as_str().ok_or("No ssoUrl in response")?;
        let proof = data["proofKey"].as_str().ok_or("No proofKey in response")?;

        eprintln!("Opening browser for SSO: {sso_url}");
        let _ = Command::new("xdg-open").arg(sso_url).spawn();
        let token = tokio::task::spawn_blocking(move || wait_for_token(listener))
            .await
            .map_err(|e| e.to_string())??;

        Self::finish(
            http,
            base,
            c,
            json!({"AUTHENTICATOR": "EXTERNALBROWSER", "TOKEN": token, "PROOF_KEY": proof}),
        )
        .await
    }

    /// Password login with a TOTP passcode read from the terminal.
    pub async fn login_mfa(c: &Connection) -> Result<Self, String> {
        let (_, _, base) = endpoint(c)?;
        let password = c
            .get("password")
            .ok_or("Missing 'password' (set SNOWFLAKE_CONNECTIONS_<NAME>_PASSWORD)")?;
        let passcode = match c.get("passcode") {
            Some(p) => p.clone(),
            None => {
                eprint!("MFA passcode: ");
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|e| e.to_string())?;
                line.trim().to_string()
            }
        };
        Self::finish(
            reqwest::Client::new(),
            base,
            c,
            json!({"AUTHENTICATOR": "USERNAME_PASSWORD_MFA", "PASSWORD": password, "PASSCODE": passcode, "EXT_AUTHN_DUO_METHOD": "passcode"}),
        )
        .await
    }

    async fn finish(
        http: reqwest::Client,
        base: String,
        c: &Connection,
        extra: Value,
    ) -> Result<Self, String> {
        let (name, user, _) = endpoint(c)?;
        let get = |k: &str| c.get(k).map(String::as_str);
        let mut data = json!({
            "ACCOUNT_NAME": name, "LOGIN_NAME": user,
            "CLIENT_APP_ID": APP.0, "CLIENT_APP_VERSION": APP.1,
        });
        data.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let mut query = vec![("request_id", rid())];
        for (k, p) in [
            ("warehouse", "warehouse"),
            ("roleName", "role"),
            ("databaseName", "database"),
            ("schemaName", "schema"),
        ] {
            if let Some(v) = get(p) {
                query.push((k, v.to_string()));
            }
        }
        let login: Value = http
            .post(format!("{base}/session/v1/login-request"))
            .query(&query)
            .json(&json!({ "data": data }))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let d = check(&login)?;
        let token = d["token"].as_str().ok_or("No session token")?.to_string();
        let ttl = d["validityInSeconds"].as_u64().unwrap_or(3600);
        save_cache(c, &token, now() + ttl);
        Ok(Self { http, base, token })
    }

    /// Reuses a cached session token while it is valid, otherwise logs in again.
    pub async fn connect(c: &Connection, mfa: bool) -> Result<Self, String> {
        if let Some(token) = load_cache(c) {
            let (_, _, base) = endpoint(c)?;
            let s = Self {
                http: reqwest::Client::new(),
                base,
                token,
            };
            if s.exec("SELECT 1").await.is_ok() {
                return Ok(s);
            }
        }
        if mfa {
            Self::login_mfa(c).await
        } else {
            Self::login(c).await
        }
    }

    fn auth(&self) -> String {
        format!("Snowflake Token=\"{}\"", self.token)
    }

    /// Runs a statement and returns the row count.
    pub async fn exec(&self, sql: &str) -> Result<usize, String> {
        let v: Value = self
            .http
            .post(format!(
                "{}/queries/v1/query-request?requestId={}",
                self.base,
                rid()
            ))
            .header("Authorization", self.auth())
            .json(&json!({"sqlText": sql, "asyncExec": false, "sequenceId": 1}))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let d = check(&v)?;
        Ok(d["total"].as_u64().map_or_else(
            || d["rowset"].as_array().map_or(0, Vec::len),
            |n| n as usize,
        ))
    }
}
