# Editor integration

Plasma can run as a headless coding agent behind an editor that supports the [Agent Client Protocol](https://agentclientprotocol.com/).

## Connect your provider first

Authenticate before the editor starts Plasma:

```sh
plasma connect openrouter --api-key <key>
```

You can also run `plasma`, open `/connect`, and complete interactive sign-in once.

## Start the protocol server

Configure the editor to start:

```sh
plasma acp
```

The editor launches the process and communicates through standard input and output. It supplies the workspace root used by Plasma's file and shell tools.

## Understand the authority boundary

The editor presents the conversation, but the Plasma process still owns its local workspace tools. Start it only for projects where those capabilities are appropriate.

See [providers and credentials](/guide/credentials) for credential storage and unattended environments. See the [command-line reference](/reference/command-line) for the exact command.
