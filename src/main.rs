use anyhow::{Context, Result};
use clap::Parser;
use inquire::Select;
use ssh2_config::{ParseRule, SshConfig, SshParserResult};
use std::env;
use std::fmt::Display;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(version)]
pub struct Args {
    #[arg(short, long)]
    config: Option<String>,
}

struct HostEntry {
    label: String,
    host: String,
    user: Option<String>,
    port: Option<u16>,
}

impl HostEntry {
    fn get_ssh_cmd(self) -> String {
        let cmd = if let Some(user) = self.user {
            format!("ssh {}@{}", user, self.label)
        } else {
            format!("ssh {}", self.label)
        };

        if let Some(port) = self.port {
            cmd + &format!(" -p {}", port)
        } else {
            cmd
        }
    }
}

impl Display for HostEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.label, self.host)
    }
}

fn get_hosts(config: &SshConfig) -> Vec<HostEntry> {
    config
        .get_hosts()
        .iter()
        .filter_map(|host| {
            let target = host.pattern.first()?;

            if target.pattern == "*" {
                return None;
            }

            Some(HostEntry {
                label: target.pattern.clone(),
                host: host.params.host_name.clone()?,
                user: host.params.user.clone(),
                port: host.params.port,
            })
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

    let hosts = get_hosts(&config);
    let host = Select::new("Host:", hosts).prompt()?;

    let ssh_cmd = host.get_ssh_cmd();

    let mut child = Command::new("bash")
        .arg("-c")
        .arg(ssh_cmd)
        .spawn()
        .context("Failed to spawn ssh command")?;

    child.wait()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_cmd_format_no_user_no_port() {
        let host = HostEntry {
            host: String::from("127.0.0.1"),
            label: String::from("localhost"),
            user: None,
            port: None,
        };

        assert_eq!(host.get_ssh_cmd(), "ssh localhost");
    }

    #[test]
    fn ssh_cmd_format_with_user() {
        let host = HostEntry {
            host: String::from("127.0.0.1"),
            label: String::from("localhost"),
            user: Some(String::from("root")),
            port: None,
        };

        assert_eq!(host.get_ssh_cmd(), "ssh root@localhost");
    }

    #[test]
    fn ssh_cmd_format_with_port() {
        let host = HostEntry {
            host: String::from("127.0.0.1"),
            label: String::from("localhost"),
            user: None,
            port: Some(666),
        };

        assert_eq!(host.get_ssh_cmd(), "ssh localhost -p 666");
    }

    #[test]
    fn ssh_cmd_format_with_user_and_port() {
        let host = HostEntry {
            host: String::from("127.0.0.1"),
            label: String::from("localhost"),
            user: Some(String::from("root")),
            port: Some(666),
        };

        assert_eq!(host.get_ssh_cmd(), "ssh root@localhost -p 666");
    }
}
