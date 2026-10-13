# bbpipelinewait

A small CLI for listing Bitbucket Pipelines runs and waiting for them to finish, so you don't have to babysit the Bitbucket UI.

Ships two binaries:

- `bbpipelinewait` — scriptable CLI (`list` / `wait` / `init`).
- `bbpw-ui` — interactive version: lets you pick a running pipeline from a list, then optionally sends a desktop notification and/or runs a shell command when it finishes.

## Installation

### Homebrew (macOS/Linux)

```sh
brew tap scottmmjackson/sj
brew install bbpipelinewait
```

### Linux packages

`.deb` and `.rpm` packages for `amd64`/`arm64` are attached to each [GitHub release](https://github.com/scottmmjackson/bbpipelinewait/releases).

### Manual download

Prebuilt archives for macOS (Intel/Apple Silicon), Linux (x86_64/aarch64), and Windows (x86_64) are attached to each [GitHub release](https://github.com/scottmmjackson/bbpipelinewait/releases). Each archive contains both `bbpipelinewait` and `bbpw-ui`.

## Configuration

Both binaries share one config file. Create it with:

```sh
bbpipelinewait init
```

This writes a template to your OS config directory and prints the path. Fill it in:

```json
{
  "username": "you@yourcompany.com",
  "api_token": "<your Bitbucket-scoped Atlassian API token>"
}
```

- `username` is your Atlassian account email, or the literal string `x-bitbucket-api-token-auth`.
- `api_token` must be a **scoped Atlassian API token**, not a Bitbucket app password — app passwords are deprecated and no longer work. Create one at [id.atlassian.com](https://id.atlassian.com) under *Security → Create and manage API tokens → Create API token with scopes*, selecting **Bitbucket** as the app, with at least the `read:pipeline:bitbucket` and `read:repository:bitbucket` scopes.
- `default_on_complete` (optional) is a shell command `bbpw-ui` offers to run on completion. The prompt shows it as `Run a command on completion? [<command>] (ESC skips)`: press Enter to accept it, type a different command to override it, or press ESC to skip.

## Usage

`workspace` and `repo` can be passed as flags or via `BITBUCKET_WORKSPACE` / `BITBUCKET_REPOSITORY` env vars.

```sh
# List currently running/pending pipelines
bbpipelinewait list --workspace myteam --repo myrepo

# Wait for all currently running pipelines to finish
bbpipelinewait wait --workspace myteam --repo myrepo

# Wait for specific pipeline build numbers (repeat -p for more than one)
bbpipelinewait wait --workspace myteam --repo myrepo -p 1234 -p 1235
```

### Interactive UI

```sh
bbpw-ui --workspace myteam --repo myrepo
```

Prompts you to pick a running pipeline (or pass `-p <build-number>` to skip the prompt), then waits for it to finish. On completion it can optionally send a desktop notification and/or run a shell command, with `PIPELINE_STATE` and `PIPELINE_BUILD_NUMBER` set in its environment.

## License

GPL-3.0
