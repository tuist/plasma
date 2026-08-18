# Plasma

🪐 Fast, simple, and extensible coding agents for your terminal.

Plasma is designed both as an end product and as a modular foundation: use it directly, or reuse its internals to build your own agents and tools. It is built for maximum portability across platforms, including [WebAssembly](https://webassembly.org/).

## Install

Install Plasma globally with [mise](https://mise.jdx.dev/):

```sh
mise use -g github:tuist/plasma
```

Run `plasma`, then type `/connect` and follow the sign-in flow. Once connected, type a request and press Enter. Use `/` to browse the available commands.

## Headless use

For scripts and continuous integration, run one task without the terminal interface:

```sh
plasma exec "Run the test suite and summarize any failures"
```

`plasma -p "..."` is a shorthand, and a prompt can also arrive on standard input. The command uses the saved OpenRouter credential, or `PLASMA_OPENROUTER_API_KEY` when set. Pass `--verbose` to show tool activity on standard error and `--json` for a machine-readable final response.

Credentials follow the [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/), defaulting to `$XDG_CONFIG_HOME/plasma` or `~/.config/plasma`. Set `PLASMA_CREDENTIAL_DIR` to give a harness an isolated, lock-protected directory.

## Headless editor integration

Plasma can also run as a headless [Agent Client Protocol](https://agentclientprotocol.com/) backend for a compatible editor:

```sh
plasma acp
```

The editor starts this command and communicates over standard input/output. Authenticate first with `plasma connect openrouter --api-key <key>`; the ACP session uses the editor-provided workspace as the root for its `read` and `bash` tools.

## Browser embedding

The browser package separates the portable conversation state machine from the capabilities owned by its host. A page supplies both its large language model inference configuration and the complete allowlist of tools available in that environment:

```js
import {createAgent, createHttpInference} from "@tuist/plasma"

const inference = createHttpInference({
  endpoint: "/api/completions",
  model: "openrouter/auto",
})

const agent = await createAgent({
  inference,
  tools: [{
    name: "read_page",
    description: "Read the public content on this page",
    parameters: {type: "object", properties: {}},
    execute: () => document.querySelector("main").innerText,
  }],
})
```

The published [npm package registry](https://www.npmjs.com/) package is `@tuist/plasma`. It contains the generated JavaScript bindings and compiled [WebAssembly](https://webassembly.org/) binary. `mise run wasm:build` rebuilds the package and stages the same files for the Elixir site.

## Marketing site

The Phoenix site under [`web`](web) demonstrates the complete browser path. It signs developers in with OpenRouter, stores the resulting credential in an encrypted application session that page JavaScript cannot read, and proxies provider-neutral completion requests from the page. Its agent can call only the page capabilities registered in `web/assets/js/app.js`; it cannot inherit the terminal application's filesystem or shell tools.

```sh
cd web
mix setup
mix phx.server
```

The development address and PostgreSQL database receive one stable numeric suffix per Git worktree, allowing multiple copies to run concurrently. The server prints its actual address at startup.

## License

Plasma is available under the [MIT License](LICENSE.md).
