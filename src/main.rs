use anyhow::{Context, Result, anyhow};
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

    #[arg(short, long, help = "Filter by subnet")]
    subnet: Option<String>,

    #[arg(short, long, help = "Override user")]
    user: Option<String>,
}

struct HostMapping {
    label: String,
    address: String,
    user: Option<String>,
}

impl HostMapping {
    fn ssh_cmd(self) -> String {
        if let Some(user) = self.user {
            format!("ssh {}@{}", user, self.label)
        } else {
            format!("ssh {}", self.label)
        }
    }
}

impl Display for HostMapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.label, self.address)
    }
}

fn get_hosts(config: &SshConfig, args: &Args) -> Vec<HostMapping> {
    config
        .get_hosts()
        .iter()
        .filter_map(|host| {
            let target = host.pattern.first()?;
            let host_name = host.params.host_name.clone()?;

            if let Some(subnet) = &args.subnet
                && !host_name.starts_with(subnet)
            {
                return None;
            }

            if target.pattern == "*" {
                return None;
            }

            Some(HostMapping {
                label: target.pattern.clone(),
                address: host_name,
                user: args.user.clone(),
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

    let hosts = get_hosts(&config, &args);

    if hosts.is_empty() {
        return Err(anyhow!("No hosts found"));
    }

    let host = Select::new("Host:", hosts).prompt()?;

    let ssh_cmd = host.ssh_cmd();

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
    fn ssh_cmd_format_with_user() {
        let host = HostMapping {
            label: String::from("localhost"),
            address: String::from("127.0.0.1"),
            user: Some(String::from("root")),
        };

        assert_eq!(host.ssh_cmd(), "ssh root@localhost");
    }

    #[test]
    fn ssh_cmd_format_no_user() {
        let host = HostMapping {
            label: String::from("localhost"),
            address: String::from("127.0.0.1"),
            user: None,
        };

        assert_eq!(host.ssh_cmd(), "ssh localhost");
    }
}
