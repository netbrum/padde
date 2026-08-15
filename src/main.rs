use anyhow::{Context, Result, anyhow};
use clap::Parser;
use inquire::Select;
use ssh2_config::{ParseRule, SshConfig, SshParserResult};
use std::collections::HashMap;
use std::env;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(version)]
pub struct Args {
    #[arg(short, long, help = "Use this config file")]
    config: Option<String>,

    #[arg(short, long, help = "Override user")]
    user: Option<String>,
}

struct HostMapping {
    alias: String,
    host: String,
    user_override: Option<String>,
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
        if let Some(user) = &self.user_override {
            write!(f, "{}@{}/{}", user, self.alias, self.host)
        } else {
            write!(f, "{}/{}", self.alias, self.host)
        }
    }
}

fn get_host_mappings(config: &SshConfig, args: &Args) -> Vec<HostMapping> {
    let mut hosts: HashMap<String, Vec<String>> = HashMap::new();

    for host in config.get_hosts() {
        for clause in &host.pattern {
            if clause.pattern.contains('*')
                || clause.pattern.contains('?')
                || clause.pattern.starts_with('!')
            {
                continue;
            }

            if let Some(host_name) = &host.params.host_name {
                hosts
                    .entry(host_name.clone())
                    .or_default()
                    .push(clause.pattern.clone());
            }
        }
    }

    hosts
        .into_iter()
        .flat_map(|(hostname, aliases)| {
            aliases
                .into_iter()
                .map(|alias| HostMapping {
                    alias: alias.clone(),
                    host: hostname.clone(),
                    user_override: args.user.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
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

        let hosts = get_host_mappings(
            &config,
            &Args {
                config: None,
                user: None,
            },
        );

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

        let hosts = get_host_mappings(
            &config,
            &Args {
                config: None,
                user: None,
            },
        );

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
    fn get_host_mappings_ignores_stanza_user() {
        let config = parse_config_from_str(
            r#"
Host foo
  HostName 10.0.0.1
  User bar
"#,
        );

        let hosts = get_host_mappings(
            &config,
            &Args {
                config: None,
                user: None,
            },
        );

        let no_user_overrides = hosts.iter().all(|h| h.user_override.is_none());

        assert!(no_user_overrides);
    }

    #[test]
    fn ssh_args_format_with_user() {
        let host = HostMapping {
            alias: String::from("localhost"),
            host: String::from("127.0.0.1"),
            user_override: Some(String::from("root")),
        };

        assert_eq!(host.ssh_args(), "root@localhost");
    }

    #[test]
    fn ssh_args_format_no_user() {
        let host = HostMapping {
            alias: String::from("localhost"),
            host: String::from("127.0.0.1"),
            user_override: None,
        };

        assert_eq!(host.ssh_args(), "localhost");
    }
}
