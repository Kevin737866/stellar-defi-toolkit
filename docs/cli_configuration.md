# CLI Configuration

The `stellar-defi-cli` binary supports named configuration profiles, so you
can keep separate settings for different networks (mainnet, testnet, local)
and wallets without passing the same flags on every invocation.

## Storage

Configuration is stored at:

```
~/.stellar-defi-toolkit/config.toml
```

The file holds every profile you've created plus a pointer to the currently
active one:

```toml
active_profile = "testnet"

[profiles.default]

[profiles.testnet]
network = "testnet"
rpc_url = "https://soroban-testnet.stellar.org"
wallet = "GABC...XYZ"

[profiles.mainnet]
network = "mainnet"
rpc_url = "https://soroban-mainnet.stellar.org"
wallet = "GDEF...UVW"
```

Each profile is a free-form set of string key/value pairs — there's no fixed
schema, so you can store whatever settings are useful (`network`, `wallet`,
`rpc_url`, etc.).

## Commands

### `config set <key> <value>`

Sets a key/value pair in the active profile, creating the profile file if it
doesn't exist yet.

```sh
stellar-defi-cli config set network testnet
stellar-defi-cli config set wallet GABC...XYZ
```

### `config get <key>`

Reads a value from the active profile.

```sh
stellar-defi-cli config get network
```

### `config profile <name>`

Switches the active profile. If the named profile doesn't exist yet, it's
created automatically.

```sh
stellar-defi-cli config profile mainnet
```

### `config profiles`

Lists every known profile, marking the currently active one with `*`.

```sh
stellar-defi-cli config profiles
#   default
# * mainnet
#   testnet
```

## Typical workflow

```sh
# Set up a testnet profile.
stellar-defi-cli config profile testnet
stellar-defi-cli config set network testnet
stellar-defi-cli config set rpc_url https://soroban-testnet.stellar.org

# Set up a separate mainnet profile.
stellar-defi-cli config profile mainnet
stellar-defi-cli config set network mainnet
stellar-defi-cli config set rpc_url https://soroban-mainnet.stellar.org

# Switch back to testnet whenever you need it.
stellar-defi-cli config profile testnet
stellar-defi-cli config get rpc_url
```
