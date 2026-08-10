# Plasma

🪐 Fast, simple, and extensible coding agents for your terminal.

Plasma is designed both as an end product and as a modular foundation: use it directly, or reuse its internals to build your own agents and tools. It is built for maximum portability across platforms, including [WebAssembly](https://webassembly.org/).

## Install

Install Plasma globally with [mise](https://mise.jdx.dev/):

```sh
mise use -g github:tuist/plasma
```

Run `plasma`, then type `/connect` and follow the sign-in flow. Once connected, type a request and press Enter. Use `/` to browse the available commands.

## Headless editor integration

Plasma can also run as a headless [Agent Client Protocol](https://agentclientprotocol.com/) backend for a compatible editor:

```sh
plasma acp
```

The editor starts this command and communicates over standard input/output. Authenticate first with `plasma connect openrouter --api-key <key>`; the ACP session uses the editor-provided workspace as the root for its `read` and `bash` tools.

## License

Plasma is available under the [MIT License](LICENSE.md).
