# Command-line reference

## `plasma`

Start the interactive terminal coding agent in the current directory.

```sh
plasma
```

Use `-p` or `--prompt` to submit one task without opening the interface:

```sh
plasma -p "Review the current changes"
```

## `plasma exec`

Run one coding task and print the final response.

```text
plasma exec [OPTIONS] [PROMPT]
```

| Option | Meaning |
| --- | --- |
| `--api-key <KEY>` | Use an OpenRouter key for this invocation |
| `--cwd <PATH>` | Run tools from another directory |
| `-v`, `--verbose` | Print tool activity to standard error |
| `--json` | Print a machine-readable final response |
| `--color <auto|always|never>` | Control terminal color |

When `PROMPT` is omitted, Plasma reads it from standard input.

## `plasma connect`

Save an OpenRouter key for later terminal and headless sessions:

```sh
plasma connect openrouter --api-key <KEY>
```

Prefer `PLASMA_OPENROUTER_API_KEY` for unattended scripts so the key is not present in shell history.

## `plasma acp`

Run Plasma as an [Agent Client Protocol](https://agentclientprotocol.com/) server over standard input and output:

```sh
plasma acp
```

The client provides the workspace root. Connect OpenRouter before starting the server.
