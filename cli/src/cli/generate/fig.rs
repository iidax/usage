use std::fmt::Debug;
use std::path::PathBuf;

use clap::Args;

use usage::{Spec, SpecArg, SpecCommand, SpecFlag};

use crate::cli::generate;

#[derive(Debug, Args)]
#[clap()]
pub struct Fig {
    /// usage spec file or script with usage shebang
    #[clap(short, long)]
    file: Option<PathBuf>,

    /// raw string spec input
    #[clap(short, long, required_unless_present = "file", overrides_with = "file")]
    spec: Option<String>,
}

impl Fig {
    pub fn run(&self) -> miette::Result<()> {
        let spec = generate::file_or_spec(&self.file, &self.spec)?;
        self.generate_fig_script(&spec)?;

        Ok(())
    }

    fn generate_fig_script(&self, spec: &Spec) -> miette::Result<()> {
        let mut script = String::new();

        // ヘッダー情報を追加
        script.push_str("const completionSpec: Fig.Spec = {\n");
        script.push_str(&format!("  name: \"{}\",\n", spec.name));
        script.push_str(&format!(
            "  description: `{}`,\n",
            Self::escape_string(spec.about.as_deref().unwrap_or(""))
        ));

        // サブコマンドを追加
        script.push_str("  subcommands: [\n");
        for (_, subcmd) in &spec.cmd.subcommands {
            self.add_subcommand_to_script(&mut script, subcmd, 2)?;
        }
        script.push_str("  ],\n");

        // オプションを追加
        script.push_str("  options: [\n");
        for flag in &spec.cmd.flags {
            self.add_flag_to_script(&mut script, flag, 2)?;
        }
        script.push_str("  ],\n");

        script.push_str("};\n");
        script.push_str("export default completionSpec;\n");

        println!("{}", script);
        Ok(())
    }

    fn add_subcommand_to_script(
        &self,
        script: &mut String,
        cmd: &SpecCommand,
        indent: usize,
    ) -> miette::Result<()> {
        let indent_str = "  ".repeat(indent);
        let display_name = if !cmd.aliases.is_empty() {
            format!("  displayName: \"{}\",\n{}", cmd.name, indent_str)
        } else {
            String::new()
        };
        let names = if !cmd.aliases.is_empty() {
            format!(
                "  name: [\"{}\", {}],\n{}",
                cmd.name,
                cmd.aliases
                    .iter()
                    .map(|a| format!("\"{}\"", a))
                    .collect::<Vec<_>>()
                    .join(", "),
                indent_str
            )
        } else {
            format!("  name: \"{}\",\n{}", cmd.name, indent_str)
        };
        let alias_names = if !cmd.aliases.is_empty() {
            format!(
                " [aliases: {}]",
                cmd.aliases
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            String::new()
        };

        script.push_str(&format!(
            "{}{{\n{}{}{}  description: `{}{}`,\n",
            indent_str,
            indent_str,
            display_name,
            names,
            Self::escape_string(cmd.help.as_deref().unwrap_or("")),
            alias_names
        ));
        // オプションを追加
        if cmd.hide {
            script.push_str(&format!("{}  hidden: true, \n", indent_str));
        }
        // サブコマンドの引数を追加
        if !cmd.args.is_empty() {
            script.push_str(&format!("{}  args: [\n", indent_str));
            for arg in &cmd.args {
                self.add_arg_to_script(script, arg, indent + 2)?;
            }
            script.push_str(&format!("{}  ],\n", indent_str));
        }
        // サブコマンドのオプションを追加
        if !cmd.flags.is_empty() {
            script.push_str(&format!("{}  options: [\n", indent_str));
            for flag in &cmd.flags {
                self.add_flag_to_script(script, flag, indent + 2)?;
            }
            script.push_str(&format!("{}  ],\n", indent_str));
        }
        // サブコマンドのサブコマンドを追加
        if !cmd.subcommands.is_empty() {
            script.push_str(&format!("{}  subcommands: [\n", indent_str));
            for (_, subcmd) in &cmd.subcommands {
                self.add_subcommand_to_script(script, subcmd, indent + 2)?;
            }
            script.push_str(&format!("{}  ], \n", indent_str));
        }

        script.push_str(&format!("{}}},\n", indent_str));
        Ok(())
    }

    fn add_flag_to_script(
        &self,
        script: &mut String,
        flag: &SpecFlag,
        indent: usize,
    ) -> miette::Result<()> {
        let indent_str = "  ".repeat(indent);
        script.push_str(&format!(
            "{}{{\n{}  name: [{}],\n",
            indent_str,
            indent_str,
            flag.short
                .iter()
                .map(|s| format!("\"-{}\"", s))
                .chain(flag.long.iter().map(|l| format!("\"--{}\"", l)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if flag.help.is_some() {
            script.push_str(&format!(
                "{}  description: `{}`, \n",
                indent_str,
                Self::escape_string(flag.help.as_deref().unwrap_or(""))
            ));
        }
        if flag.arg.is_some() {
            script.push_str(&format!("{}  args: [\n", indent_str));
            // フラグの引数の詳細を追加
            if let Some(arg) = flag.arg.as_ref() {
                self.add_arg_to_script(script, arg, indent + 2)?;
            }
            script.push_str(&format!("{}  ],\n", indent_str));
        }
        if flag.global {
            script.push_str(&format!("{}  isPersistent: true, \n", indent_str));
        }
        if flag.hide {
            script.push_str(&format!("{}  hidden: true, \n", indent_str));
        }

        script.push_str(&format!("{}}},\n", indent_str));
        Ok(())
    }

    fn add_arg_to_script(
        &self,
        script: &mut String,
        arg: &SpecArg,
        indent: usize,
    ) -> miette::Result<()> {
        let indent_str = "  ".repeat(indent);
        script.push_str(&format!(
            "{}{{\n{}  name: \"{}\",\n",
            indent_str, indent_str, arg.name,
        ));
        // 引数の追加情報（必須かどうかなど）を追加
        if arg.help.is_some() {
            script.push_str(&format!(
                "{}  description: `{}`, \n",
                indent_str,
                Self::escape_string(arg.help.as_deref().unwrap_or(""))
            ));
        }
        if !arg.required {
            script.push_str(&format!("{}  isOptional: true, \n", indent_str));
        }
        if arg.var {
            script.push_str(&format!("{}  isVariadic: true,\n", indent_str));
        }
        if let Some(default) = &arg.default {
            script.push_str(&format!("{}  default: \"{}\",\n", indent_str, default));
        }
        script.push_str(&format!("{}}},\n", indent_str));
        Ok(())
    }

    // 文字列をエスケープする関数
    fn escape_string(s: &str) -> String {
        s.replace('`', "\\`").replace('"', "\\\"")
    }
}
