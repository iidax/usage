# Usage (Unofficial Fork with Fig Support)

This is an unofficial, forked version of [Usage](https://usage.jdx.dev/), a spec and CLI for defining CLI tools. This fork extends the original project by adding support for [Fig](https://fig.io/), a CLI tool with its own unique completion system. Please note that this is not affiliated with or endorsed by the original Usage project or Fig.

## New Feature:

- Generate TypeScript completion scripts for Fig from [KDL](https://kdl.dev/) files

This project aims to bridge the gap between various CLI definition formats and Fig's completion system, which requires TypeScript scripts instead of traditional shell scripts.

You can generate a TypeScript script using the following command:
```shell
$ cargo run -- complete-word --shell fig --file ./examples/mise.usage.kdl
```

Please note that this feature is currently under development and may be subject to changes.

## Original Features:

- Generate shell completions for any shell
- Generate --help docs, markdown, and manpage documentation
- Write scripts in bash or any other language with modern arg parsing, help, and completions

> [!WARNING]
> This is beta software and may have breaking changes both with the CLI and schema design. You've been warned.

For more information about the original project, please visit [usage.jdx.dev](https://usage.jdx.dev/).