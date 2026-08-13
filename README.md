# Padde

Dead simple ssh_config TUI for linux written in ~100 lines.

### Install

```sh
cargo install --git https://github.com/netbrum/padde
```

### Usage

By default, `padde` looks for the config file in `$HOME/.ssh/config`, usage is as simple as just running the command:

```sh
padde
```

You can however use a different config file, by setting the `--config` option:

```sh
padde --config ~/.ssh/second_config
```
