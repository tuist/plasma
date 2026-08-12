# Rust crate architecture

Plasma is a workspace of focused crates. Applications can depend on the narrowest layer that matches the host they are building.

## Foundation crates

| Crate | Responsibility |
| --- | --- |
| `plasma-protocol` | Provider-neutral messages, roles, and tool-call identifiers. |
| `plasma-inference` | Provider and tool-definition traits shared by session implementations. |
| `plasma-session` | Conversation history, host-driven state transitions, and the synchronous native agent loop. |

The host-driven `HostSession` is the portable core used when inference and tools execute outside Rust. It produces a completion request, accepts one assistant response, requests tool outputs when needed, and validates every transition.

## Feature crates

| Crate | Responsibility |
| --- | --- |
| `plasma-tools` | Native file and shell tools plus their model-visible definitions. |
| `plasma-openrouter` | OpenRouter transport and credential handling for native hosts. |
| `plasma-wasm` | WebAssembly bindings around the host-driven session. |

These crates add capabilities without making them assumptions of the portable session. A browser host can use `plasma-wasm` without linking native file or shell tools.

## Application crate

The `plasma` crate assembles the terminal coding agent, one-shot execution, provider sign-in, workspace tools, rich terminal presentation, and [Agent Client Protocol](https://agentclientprotocol.com/) integration.

Use it as an architectural example when building another application, but keep host-specific policy in your application layer.

## Choose a starting point

- Use `@tuist/plasma` and the [browser quickstart](/guide/browser) for a web application.
- Use `plasma-session::HostSession` and the [host interface](/reference/host-interface) for a custom asynchronous host.
- Use `plasma-session::Session` with an `InferenceProvider` and `ToolDispatcher` for a synchronous native host.

Review [how Plasma works](/guide/how-plasma-works) before coupling a new host to a lower-level crate.
