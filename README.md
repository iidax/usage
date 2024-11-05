# Usage

This is forked version of [Usage](https://usage.jdx.dev/), a spec and CLI for defining CLI tools. This fork extends the original project by adding support for [Fig](https://fig.io/)
## New Feature:

- Generate TypeScript completion scripts for Fig from [KDL](https://kdl.dev/) files

This project aims to bridge the gap between various CLI definition formats and Fig's completion system, which requires TypeScript scripts instead of shell scripts.

You can generate a TypeScript script using the following command:
```shell
$ cargo run -- generate fig --file ./examples/mise.usage.kdl
```

For more information about the original project, please visit [usage.jdx.dev](https://usage.jdx.dev/).