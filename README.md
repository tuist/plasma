# Plasma

🪐 Fast, simple, and extensible coding agents for your terminal.

Plasma is designed both as an end product and as a modular foundation: use it directly, or reuse its internals to build your own agents and tools. It is built for maximum portability across platforms, including [WebAssembly](https://webassembly.org/).

## Install

Install Plasma globally with [mise](https://mise.jdx.dev/):

```sh
mise use -g github:tuist/plasma
```

Run `plasma`, then type `/connect` and follow the sign-in flow. Once connected, type a request and press Enter. Use `/` to browse the available commands.

If you already signed in to Pi with OpenRouter, import its access credential without printing it:

```sh
plasma import-pi-credentials
```

## Headless use

For scripts and continuous integration, run one task without the terminal interface:

```sh
plasma exec "Run the test suite and summarize any failures"
```

`plasma -p "..."` is a shorthand, and a prompt can also arrive on standard input. The command uses the saved OpenRouter credential, or `PLASMA_OPENROUTER_API_KEY` when set. Pass `--verbose` to show tool activity on standard error and `--json` for a machine-readable final response.

Credentials follow the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/), defaulting to `$XDG_CONFIG_HOME/plasma` or `~/.config/plasma`. Set `PLASMA_CREDENTIAL_DIR` to give a harness an isolated, lock-protected directory. The importer reads Pi's usual `~/.pi/agent/auth.json` source; set `PLASMA_PI_AUTH_FILE` to use another source file.

## Headless editor integration

Plasma can also run as a headless [Agent Client Protocol](https://agentclientprotocol.com/) backend for a compatible editor:

```sh
plasma acp
```

The editor starts this command and communicates over standard input/output. Authenticate first with `plasma connect openrouter --api-key <key>`; the ACP session uses the editor-provided workspace as the root for its `read` and `bash` tools.

## License

Plasma is available under the [MIT License](LICENSE.md).
