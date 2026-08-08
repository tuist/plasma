# Plasma

An early Rust foundation for a coding agent, with a terminal user interface and an OpenRouter inference provider.

## Run

```sh
mise exec rust@1.91 -- cargo run -p plasma -- connect openrouter --api-key "$OPENROUTER_API_KEY"
mise exec rust@1.91 -- cargo run -p plasma
```

Type `/` in the terminal interface to see available commands. Enter `/connect` to add an OpenRouter API key, then type a prompt to send it to the selected model.

The stored key uses the operating-system appropriate configuration directory and is restricted to the current user on Unix-like systems.
