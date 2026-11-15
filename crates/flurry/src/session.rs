use crate::config::{self, Connection};
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

/// Result rows as strings, with column names lowercased.
pub struct Rows {
    pub cols: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
}

impl Rows {
    pub fn get(&self, row: usize, col: &str) -> Option<String> {
        let i = self.cols.iter().position(|c| c == col)?;
        self.rows[row][i].clone()
    }
}

fn rid() -> String {
    Uuid::new_v4().to_string()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
    Some(config::cache_dir()?.join(format!("session-{key}.json")))
}

fn load_cache(c: &Connection) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(cache_path(c)?).ok()?).ok()?;
    (v["expires"].as_u64()? > now() + 60).then(|| v["token"].as_str().map(String::from))?
}

// Best effort: a cache failure must never break the run.
fn save_cache(c: &Connection, token: &str, expires: u64) {
    if let Some(path) = cache_path(c) {
        config::write_private(
            &path,
            json!({"token": token, "expires": expires})
                .to_string()
                .as_bytes(),
        );
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

async fn post(req: reqwest::RequestBuilder) -> Result<Value, String> {
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Cannot reach Snowflake: {e}"))?;
    let status = resp.status();
    resp.json().await.map_err(|_| {
        format!("Unexpected response from Snowflake (HTTP {status}); check the account identifier")
    })
}

impl Session {
    /// Reuses a cached session token while it is valid, otherwise logs in with the configured authenticator.
    pub async fn connect(c: &Connection) -> Result<Self, String> {
        let auth = c
            .get("authenticator")
            .map_or(String::new(), |a| a.to_ascii_lowercase());
        let cacheable = auth == "externalbrowser" || auth == "username_password_mfa";
        if cacheable && let Some(token) = load_cache(c) {
            let s = Self {
                http: reqwest::Client::new(),
                base: endpoint(c)?.2,
                token,
            };
            if s.query("SELECT 1").await.is_ok() {
                return Ok(s);
            }
        }
        match auth.as_str() {
            "externalbrowser" => Self::login_browser(c).await,
            "username_password_mfa" => Self::login_mfa(c).await,
            _ => Self::login_key(c).await,
        }
    }

    async fn login_browser(c: &Connection) -> Result<Self, String> {
        let (name, user, base) = endpoint(c)?;
        let http = reqwest::Client::new();
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let auth = post(
            http.post(format!(
                "{base}/session/authenticator-request?request_id={}",
                rid()
            ))
            .json(&json!({"data": {
                "ACCOUNT_NAME": name, "LOGIN_NAME": user,
                "AUTHENTICATOR": "EXTERNALBROWSER", "BROWSER_MODE_REDIRECT_PORT": port.to_string(),
                "CLIENT_APP_ID": APP.0, "CLIENT_APP_VERSION": APP.1,
            }})),
        )
        .await?;
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

    /// Password login with a TOTP passcode from the config or the terminal.
    async fn login_mfa(c: &Connection) -> Result<Self, String> {
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
            endpoint(c)?.2,
            c,
            json!({"AUTHENTICATOR": "USERNAME_PASSWORD_MFA", "PASSWORD": password, "PASSCODE": passcode, "EXT_AUTHN_DUO_METHOD": "passcode"}),
        )
        .await
    }

    async fn login_key(c: &Connection) -> Result<Self, String> {
        let (name, user, base) = endpoint(c)?;
        let path = c
            .get("private_key_file")
            .or_else(|| c.get("private_key_path"))
            .ok_or("Missing 'private_key_file' in connection")?;
        let pem = std::fs::read_to_string(config::expand(path)).map_err(|e| e.to_string())?;
        let id = format!(
            "{}.{}",
            name.to_ascii_uppercase(),
            user.to_ascii_uppercase()
        );
        let jwt = snowflake_jwt::generate_jwt_token(&pem, &id).map_err(|e| e.to_string())?;
        Self::finish(
            reqwest::Client::new(),
            base,
            c,
            json!({"AUTHENTICATOR": "SNOWFLAKE_JWT", "TOKEN": jwt}),
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
            if let Some(v) = c.get(p) {
                query.push((k, v.to_string()));
            }
        }
        let login = post(
            http.post(format!("{base}/session/v1/login-request"))
                .query(&query)
                .json(&json!({ "data": data })),
        )
        .await?;
        let d = check(&login)?;
        let token = d["token"].as_str().ok_or("No session token")?.to_string();
        if c.get("authenticator").is_some() {
            save_cache(
                c,
                &token,
                now() + d["validityInSeconds"].as_u64().unwrap_or(3600),
            );
        }
        Ok(Self { http, base, token })
    }

    /// Runs a read-only statement and returns all rows, following result chunks.
    pub async fn query(&self, sql: &str) -> Result<Rows, String> {
        guard(sql)?;
        let v = post(
            self.http
                .post(format!(
                    "{}/queries/v1/query-request?requestId={}",
                    self.base,
                    rid()
                ))
                .header(
                    "Authorization",
                    format!("Snowflake Token=\"{}\"", self.token),
                )
                .json(&json!({"sqlText": sql, "asyncExec": false, "sequenceId": 1})),
        )
        .await?;
        let d = check(&v)?;
        let mut rows = rows_from(d);
        for chunk in d["chunks"].as_array().into_iter().flatten() {
            rows.rows.extend(self.chunk(d, chunk).await?);
        }
        Ok(rows)
    }

    async fn chunk(&self, d: &Value, chunk: &Value) -> Result<Vec<Vec<Option<String>>>, String> {
        let mut req = self
            .http
            .get(chunk["url"].as_str().ok_or("Chunk without url")?);
        if let Some(h) = d["chunkHeaders"].as_object() {
            for (k, v) in h {
                req = req.header(k, v.as_str().unwrap_or(""));
            }
        } else if let Some(key) = d["qrmk"].as_str() {
            req = req
                .header("x-amz-server-side-encryption-customer-algorithm", "AES256")
                .header("x-amz-server-side-encryption-customer-key", key);
        }
        let body = req
            .send()
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        parse_chunk(&body)
    }
}

// Chunks are comma-separated JSON arrays without the outer brackets.
fn parse_chunk(body: &str) -> Result<Vec<Vec<Option<String>>>, String> {
    let v: Value = serde_json::from_str(&format!("[{body}]")).map_err(|e| e.to_string())?;
    Ok(parse_rows(&v))
}

// Builds the first page of rows (inline `rowset`) with lowercased column names.
fn rows_from(d: &Value) -> Rows {
    let cols = d["rowtype"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| c["name"].as_str().unwrap_or("").to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    Rows {
        cols,
        rows: parse_rows(&d["rowset"]),
    }
}

fn parse_rows(v: &Value) -> Vec<Vec<Option<String>>> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            r.as_array()
                .into_iter()
                .flatten()
                .map(|c| c.as_str().map(String::from))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_allows_only_show_and_select() {
        for ok in ["SHOW ROLES", "  select 1", "show\nusers"] {
            assert!(guard(ok).is_ok(), "{ok}");
        }
        for bad in [
            "GRANT ROLE a TO b",
            "drop table t",
            "",
            "ALTER USER u SET x=1",
            "USE ROLE r",
        ] {
            assert!(guard(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn check_returns_data_or_the_snowflake_message() {
        let ok = json!({"success": true, "data": {"a": 1}});
        assert_eq!(check(&ok).unwrap()["a"], 1);
        let err = json!({"success": false, "message": "Incorrect username or password"});
        assert_eq!(check(&err).unwrap_err(), "Incorrect username or password");
        assert!(check(&json!({"success": false})).is_err());
    }

    #[test]
    fn rows_keep_nulls_and_lowercase_column_names() {
        let d = json!({
            "rowtype": [{"name": "NAME"}, {"name": "COMMENT"}],
            "rowset": [["A", null], ["B", "hi"]],
        });
        let r = rows_from(&d);
        assert_eq!(r.cols, ["name", "comment"]);
        assert_eq!(r.get(0, "name").as_deref(), Some("A"));
        assert_eq!(r.get(0, "comment"), None);
        assert_eq!(r.get(1, "comment").as_deref(), Some("hi"));
        assert_eq!(r.get(1, "missing"), None);
    }

    #[test]
    fn chunk_bodies_without_outer_brackets_are_parsed() {
        let rows = parse_chunk(r#"["a","b"],["c",null]"#).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1], [Some("c".to_string()), None]);
        assert!(parse_chunk("not json").is_err());
        assert!(parse_chunk("").unwrap().is_empty());
    }

    #[test]
    fn endpoint_lowercases_host_and_strips_region_from_name() {
        let c: Connection = [("account", "ORG-ACC.eu-west-1"), ("user", "u")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let (name, user, base) = endpoint(&c).unwrap();
        assert_eq!((name.as_str(), user.as_str()), ("ORG-ACC", "u"));
        assert_eq!(base, "https://org-acc.eu-west-1.snowflakecomputing.com");
        assert!(endpoint(&Connection::new()).is_err());
    }

    #[test]
    fn cache_path_differs_by_role_and_is_filesystem_safe() {
        let mk = |role: &str| -> Connection {
            [("account", "A.B"), ("user", "u@x.com"), ("role", role)]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let a = cache_path(&mk("R1")).unwrap();
        assert_ne!(a, cache_path(&mk("R2")).unwrap());
        let file = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            file.starts_with("session-") && !file.contains(['@', '/']) && file.ends_with(".json")
        );
    }
}
