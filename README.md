## Padde

Dead simple ssh_config TUI.

### Install

```sh
cargo install --git https://github.com/netbrum/padde
```

### Usage

By default `padde` looks for a config file in your home directory (`$HOME/.ssh/config` for unix, `%USERPROFILE%\.ssh\config` for windows).

Upon running the program, you'll be met with a selection like the below example.

```sh
? Host:
> • root@foo 10.0.0.1
  • root@bar 10.0.0.2
  • root@baz 10.0.0.3
[↑↓ to move, enter to select, type to filter]
```

The dot indicates if the host is reachable by color (green or red).

> [!NOTE]  
> Stanzas that use wildcards or negations are excluded from the selection.
