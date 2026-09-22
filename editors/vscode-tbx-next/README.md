# TBX Next VS Code syntax highlighting

This directory contains the repository-local VS Code language definition for TBX Next.

The extension is not published to the VS Code Marketplace. Opening the repository normally does not load this directory as an extension by itself.

## Run from this repository

Open `kkismd/tbx` in VS Code, select the **TBX Next Syntax Highlight** launch configuration, and start debugging (F5).

The launched Extension Development Host loads this directory through `--extensionDevelopmentPath` and opens the same repository workspace. In that window, the workspace associations in `.vscode/settings.json` apply the `tbx-next` language mode to:

- `docs/next/**/*.tbx`
- `crates/tbx-next/**/*.tbx`
- `editors/vscode-tbx-next/test/**/*.tbx`

Open `editors/vscode-tbx-next/test/highlight.tbx` to inspect the representative highlighting cases, including an unknown statement head that is classified from its source position rather than from a word list.

The same extension can also be loaded directly from a VS Code CLI invocation that supplies this directory as `--extensionDevelopmentPath`.

## Scope

This extension intentionally performs lexical and local-position-based highlighting only. It does not perform name resolution, semantic highlighting, completion, diagnostics, or other language-server functions.
