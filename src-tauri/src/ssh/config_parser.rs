use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SshHostConfig {
    pub host: String,
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub identity_file: Option<String>,
}

/// Parse ~/.ssh/config manually and return all Host aliases.
/// We parse manually because ssh2-config can only resolve specific hosts, not enumerate them.
pub fn list_ssh_hosts() -> Result<Vec<SshHostConfig>, String> {
    let config_path = dirs::home_dir()
        .ok_or("Cannot find home directory")?
        .join(".ssh/config");

    if !config_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read SSH config: {}", e))?;

    let mut hosts = Vec::new();
    let mut current: Option<SshHostConfig> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed
            .splitn(2, |c: char| c.is_whitespace() || c == '=')
            .collect();
        if parts.len() < 2 {
            continue;
        }
        let key = parts[0].to_lowercase();
        let value = parts[1].trim().to_string();

        match key.as_str() {
            "host" => {
                if let Some(h) = current.take() {
                    if !h.host.contains('*') {
                        hosts.push(h);
                    }
                }
                current = Some(SshHostConfig {
                    host: value,
                    hostname: None,
                    port: None,
                    user: None,
                    identity_file: None,
                });
            }
            "hostname" => {
                if let Some(ref mut h) = current {
                    h.hostname = Some(value);
                }
            }
            "port" => {
                if let Some(ref mut h) = current {
                    h.port = value.parse().ok();
                }
            }
            "user" => {
                if let Some(ref mut h) = current {
                    h.user = Some(value);
                }
            }
            "identityfile" => {
                if let Some(ref mut h) = current {
                    h.identity_file = Some(value);
                }
            }
            _ => {}
        }
    }
    if let Some(h) = current {
        if !h.host.contains('*') {
            hosts.push(h);
        }
    }

    Ok(hosts)
}
