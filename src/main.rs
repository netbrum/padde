use anyhow::{Context, Result, anyhow};
use clap::Parser;
use inquire::Select;
use ssh2_config::{ParseRule, SshConfig, SshParserResult};
use std::env;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

#[derive(Parser, Default)]
#[command(version)]
pub struct Args {
    #[arg(short, long, help = "Use this config file")]
    config: Option<String>,

    #[arg(short, long, help = "Override the user")]
    user: Option<String>,

    #[arg(short, long, default_value_t = 100, help = "Ping timeout")]
    timeout: u64,
}

#[derive(Debug)]
struct HostMapping {
    alias: String,
    host: String,
    user: Option<String>,
    user_override: Option<String>,
    online: bool,
}

impl HostMapping {
    fn ssh_args(&self) -> String {
        if let Some(user) = &self.user_override {
            format!("{}@{}", user, self.alias)
        } else {
            self.alias.clone()
        }
    }
}

impl Display for HostMapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.online { "Online" } else { "Offline" };

        match (&self.user, &self.user_override) {
            (Some(user), None) => write!(f, "{}@{} ({}) • {}", user, self.alias, self.host, status),
            (_, Some(user)) => write!(f, "{}@{} ({}) • {}", user, self.alias, self.host, status),
            (None, None) => write!(f, "{} ({}) • {}", self.alias, self.host, status),
        }
    }
}

fn get_host_mappings(config: &SshConfig, args: &Args) -> Vec<HostMapping> {
    let mut hosts: Vec<HostMapping> = Vec::new();

    for host in config.get_hosts() {
        for clause in &host.pattern {
            if clause.pattern.contains('*')
                || clause.pattern.contains('?')
                || clause.pattern.starts_with('!')
            {
                continue;
            }

            if let Some(host_name) = &host.params.host_name {
                let ip = if let Ok(mut addr) = (host_name.to_owned() + ":0").to_socket_addrs() {
                    addr.next().map(|next| next.ip())
                } else {
                    None
                };

                let online = if let Some(ip) = ip {
                    ping::new(ip)
                        .timeout(Duration::from_millis(args.timeout))
                        .send()
                        .is_ok()
                } else {
                    false
                };

                hosts.push(HostMapping {
                    alias: clause.pattern.clone(),
                    host: host_name.clone(),
                    user: host.params.user.clone(),
                    user_override: args.user.clone(),
                    online,
                })
            }
        }
    }

    hosts
}

fn get_config_file(args: &Args) -> Result<File> {
    if let Some(config) = &args.config {
        let path = PathBuf::from(config);
        Ok(File::open(&path)
            .with_context(|| format!("Failed to read config from \"{}\"", config))?)
    } else {
        let home_dir = env::var("HOME")?;
        let config = home_dir + "/.ssh/config";
        let path = PathBuf::from(config);

        Ok(File::open(path).context("No config file found")?)
    }
}

fn parse_config(file: File) -> SshParserResult<SshConfig> {
    let mut reader = BufReader::new(file);
    SshConfig::default().parse(&mut reader, ParseRule::STRICT)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config_file = get_config_file(&args)?;
    let config = parse_config(config_file).context("Failed to parse config file")?;

    let hosts = get_host_mappings(&config, &args);

    if hosts.is_empty() {
        return Err(anyhow!("No hosts found"));
    }

    let host = Select::new("Host:", hosts).prompt()?;

    let mut child = Command::new("ssh")
        .arg(host.ssh_args())
        .spawn()
        .context("Failed to spawn ssh command")?;

    child.wait()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn parse_config_from_str(s: &str) -> SshConfig {
        let mut tmp = NamedTempFile::new().expect("Create temporary file");
        write!(tmp, "{}", s).expect("Write temporary config");

        let file = File::open(tmp.path()).expect("Open temporary config");
        parse_config(file).expect("Parse config")
    }

    #[test]
    fn get_host_mappings_matches_basic_stanza() {
        let config = parse_config_from_str(
            r#"
Host foo
  HostName 10.0.0.1
"#,
        );

        let hosts = get_host_mappings(&config, &Args::default());

        dbg!(&hosts);

        let has_mapping = hosts
            .iter()
            .find(|h| h.alias == "foo" && h.host == "10.0.0.1")
            .is_some();

        assert!(has_mapping);
    }

    #[test]
    fn get_host_mappings_matches_multiple_stanza_hosts() {
        let config = parse_config_from_str(
            r#"
Host foo
  HostName 10.0.0.1

Host bar baz
HostName 10.0.0.2
"#,
        );

        let hosts = get_host_mappings(&config, &Args::default());

        let has_foo = hosts
            .iter()
            .find(|h| h.alias == "foo" && h.host == "10.0.0.1")
            .is_some();

        let has_bar = hosts
            .iter()
            .find(|h| h.alias == "bar" && h.host == "10.0.0.2")
            .is_some();

        let has_baz = hosts
            .iter()
            .find(|h| h.alias == "baz" && h.host == "10.0.0.2")
            .is_some();

        assert!(has_foo && has_bar && has_baz);
    }

    #[test]
    fn ssh_args_format_with_user_override() {
        let host = HostMapping {
            alias: String::from("localhost"),
            host: String::from("127.0.0.1"),
            user: None,
            user_override: Some(String::from("root")),
            online: false,
        };

        assert_eq!(host.ssh_args(), "root@localhost");
    }

    #[test]
    fn ssh_args_format_with_user_and_user_override() {
        let host = HostMapping {
            alias: String::from("localhost"),
            host: String::from("127.0.0.1"),
            user: Some(String::from("foo")),
            user_override: Some(String::from("bar")),
            online: false,
        };

        assert_eq!(host.ssh_args(), "bar@localhost");
    }

    #[test]
    fn ssh_args_format_no_user() {
        let host = HostMapping {
            alias: String::from("localhost"),
            host: String::from("127.0.0.1"),
            user: None,
            user_override: None,
            online: false,
        };

        assert_eq!(host.ssh_args(), "localhost");
    }
}
