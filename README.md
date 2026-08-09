# Plasma

🪐 A coding agent for your terminal.

## Get started

Plasma needs Rust 1.91 or later.

```sh
cargo run -p plasma
```

Type `/connect` and follow the sign-in flow. Once connected, type a request and press Enter.

Use `/` to browse the available commands.

## Install with mise

[mise](https://mise.jdx.dev/) can install Plasma directly from the release
artifacts published on [GitHub Releases](https://github.com/tuist/plasma/releases):

```sh
mise use -g github:tuist/plasma
```

This installs the latest release globally. To add Plasma to a project's
`mise.toml`, run `mise use github:tuist/plasma` from that project instead.

## Connect from a script

If you already have an OpenRouter key, you can connect without opening the terminal interface:

```sh
cargo run -p plasma -- connect openrouter --api-key "$OPENROUTER_API_KEY"
```

Your key is stored in your operating system's configuration directory and is readable only by your user account on Unix-like systems.

## License

Plasma is available under the [MIT License](LICENSE.md).
