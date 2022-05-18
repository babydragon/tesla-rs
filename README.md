- [Tesla lib](./tesla)
- [Tesla command line utility / example app](./teslac)
- [Tesla fake server (used for testing or demo mode)](./fake_server)

## Features
* influxdb: exists before I clone, but I split it out use feature, so I can cross build to aarch64.
* sqlite: default on, store data in a sqlite database.
* mqtt: default on, send data to mqtt topic.

## Build

```bash
cargo build --release
```

To cross build: (use docker/podman)

```bash
# install cross
cargo install cross

# build for arm
cross build --target aarch64-unknown-linux-gnu --release
```

For other targets, please check [cross documentation](https://github.com/cross-rs/cross#supported-targets).

## Configuration
`teslac` use `$HOME/.teslac` to configure. When you first run `teslac`, 
it will ask your tesla account username and password, 
and exchange access token and refresh token from tesla servers.
If all authentication is successful, it will save the configuration to `$HOME/.teslac`.

**Note:** currently, only support and test on Tesla account which registered on the China site. 
For other accounts, please try to change authentication endpoint domain (from .cn to .com).

After authentication process, `teslac` will write token config to `$HOME/.teslac`.

If you want to use sqlite feature, you can add sqlite config in `$HOME/.teslac`:

```toml
[sqlite]
file = "/data/tesla.db"
```

If you want to use mqtt feature, you can add mqtt config:
```toml
[mqtt]
host = "127.0.0.1"
port = 1883
username = "username"
password = "password"
topic = "vehicle/tesla"
```
**Note:** username and password are optional.

Currently, `teslac` only support one feature.