use std::collections::BTreeMap;
use std::env;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use itertools::Itertools;
use miette::IntoDiagnostic;
use once_cell::sync::Lazy;
use xx::process::check_status;
use xx::{XXError, XXResult};

use usage::{Complete, Spec, SpecArg, SpecCommand, SpecFlag};

use crate::cli::generate;

#[derive(Debug, Args)]
#[clap(visible_alias = "cw")]
pub struct CompleteWord {
    #[clap(long, value_parser = ["bash", "fish", "zsh", "fig"])]
    shell: Option<String>,

    /// user's input from the command line
    words: Vec<String>,

    /// usage spec file or script with usage shebang
    #[clap(short, long)]
    file: Option<PathBuf>,

    /// raw string spec input
    #[clap(short, long, required_unless_present = "file", overrides_with = "file")]
    spec: Option<String>,

    /// current word index
    #[clap(long, allow_hyphen_values = true)]
    cword: Option<usize>,
}

impl CompleteWord {
    pub fn run(&self) -> miette::Result<()> {
        let spec = generate::file_or_spec(&self.file, &self.spec)?;
        let choices = self.complete_word(&spec)?;
        let shell = self.shell.as_deref().unwrap_or_default();
        let any_descriptions = choices.iter().any(|(_, d)| !d.is_empty());
        if shell == "fig" {
            self.generate_fig_script(&spec)?;
        } else {
            for (c, description) in choices {
                match (any_descriptions, shell) {
                    (true, "bash") => println!("{c}"),
                    (true, "fish") => println!("{c}\t{description}"),
                    (true, "zsh") => println!("{c}\\:'{description}'"),
                    _ => println!("{c}"),
                }
            }
        }

        Ok(())
    }

    fn complete_word(&self, spec: &Spec) -> miette::Result<Vec<(String, String)>> {
        let cword = self.cword.unwrap_or(self.words.len().max(1) - 1);
        let ctoken = self.words.get(cword).cloned().unwrap_or_default();
        let words: Vec<_> = self.words.iter().take(cword).cloned().collect();

        trace!(
            "cword: {cword} ctoken: {ctoken} words: {}",
            words.iter().join(" ")
        );

        let mut ctx = tera::Context::new();
        ctx.insert("words", &self.words);
        ctx.insert("CURRENT", &cword);
        if cword > 0 {
            ctx.insert("PREV", &(cword - 1));
        }

        let parsed = usage::cli::parse(spec, &words)?;
        debug!("parsed cmd: {}", parsed.cmd.full_cmd.join(" "));
        let choices = if !parsed.cmd.subcommands.is_empty() {
            self.complete_subcommands(parsed.cmd, &ctoken)
        } else if ctoken == "-" {
            let shorts = self.complete_short_flag_names(&parsed.available_flags, "");
            let longs = self.complete_long_flag_names(&parsed.available_flags, "");
            shorts.into_iter().chain(longs).collect()
        } else if ctoken.starts_with("--") {
            self.complete_long_flag_names(&parsed.available_flags, &ctoken)
        } else if ctoken.starts_with('-') {
            self.complete_short_flag_names(&parsed.available_flags, &ctoken)
        } else if let Some(flag) = parsed.flag_awaiting_value {
            self.complete_arg(&ctx, spec, flag.arg.as_ref().unwrap(), &ctoken)?
        } else if let Some(arg) = parsed.cmd.args.get(parsed.args.len()) {
            self.complete_arg(&ctx, spec, arg, &ctoken)?
        } else {
            vec![]
        };
        Ok(choices)
    }

    fn complete_subcommands(&self, cmd: &SpecCommand, ctoken: &str) -> Vec<(String, String)> {
        trace!("complete_subcommands: {ctoken}");
        let mut choices = vec![];
        for subcommand in cmd.subcommands.values() {
            if subcommand.hide {
                continue;
            }
            choices.push((
                subcommand.name.clone(),
                subcommand.help.clone().unwrap_or_default(),
            ));
            for alias in &subcommand.aliases {
                choices.push((alias.clone(), subcommand.help.clone().unwrap_or_default()));
            }
        }
        choices
            .into_iter()
            .filter(|(c, _)| c.starts_with(ctoken))
            .sorted()
            .collect()
    }

    fn complete_long_flag_names(
        &self,
        flags: &BTreeMap<String, SpecFlag>,
        ctoken: &str,
    ) -> Vec<(String, String)> {
        debug!("complete_long_flag_names: {ctoken}");
        trace!("flags: {}", flags.keys().join(", "));
        let ctoken = ctoken.strip_prefix("--").unwrap_or(ctoken);
        flags
            .values()
            .filter(|f| !f.hide)
            .flat_map(|f| &f.long)
            .unique()
            .filter(|c| c.starts_with(ctoken))
            // TODO: get flag description
            .map(|c| (format!("--{c}"), String::new()))
            .sorted()
            .collect()
    }

    fn complete_short_flag_names(
        &self,
        flags: &BTreeMap<String, SpecFlag>,
        ctoken: &str,
    ) -> Vec<(String, String)> {
        debug!("complete_short_flag_names: {ctoken}");
        let cur = ctoken.chars().nth(1);
        flags
            .values()
            .filter(|f| !f.hide)
            .flat_map(|f| &f.short)
            .unique()
            .filter(|c| cur.is_none() || cur == Some(**c))
            // TODO: get flag description
            .map(|c| (format!("-{c}"), String::new()))
            .sorted()
            .collect()
    }

    fn complete_builtin(&self, type_: &str, ctoken: &str) -> Vec<(String, String)> {
        let names = match (type_, env::current_dir()) {
            ("path", Ok(cwd)) => self.complete_path(&cwd, ctoken, |_| true),
            ("dir", Ok(cwd)) => self.complete_path(&cwd, ctoken, |p| p.is_dir()),
            ("file", Ok(cwd)) => self.complete_path(&cwd, ctoken, |p| p.is_file()),
            _ => vec![],
        };
        names.into_iter().map(|n| (n, String::new())).collect()
    }

    fn complete_arg(
        &self,
        ctx: &tera::Context,
        spec: &Spec,
        arg: &SpecArg,
        ctoken: &str,
    ) -> miette::Result<Vec<(String, String)>> {
        static EMPTY_COMPL: Lazy<Complete> = Lazy::new(Complete::default);

        trace!("complete_arg: {arg} {ctoken}");
        let name = arg.name.to_lowercase();
        let complete = spec.complete.get(&name).unwrap_or(&EMPTY_COMPL);
        let type_ = complete.type_.as_ref().unwrap_or(&name);

        let builtin = self.complete_builtin(type_, ctoken);
        if !builtin.is_empty() {
            return Ok(builtin);
        }

        if let Some(run) = &complete.run {
            let run = tera::Tera::one_off(run, ctx, false).into_diagnostic()?;
            trace!("run: {run}");
            let stdout = sh(&run)?;
            // trace!("stdout: {stdout}");
            return Ok(stdout
                .lines()
                .filter(|l| l.starts_with(ctoken))
                // TODO: allow a description somehow
                .map(|l| (l.to_string(), String::new()))
                .collect());
        }

        Ok(vec![])
    }

    fn complete_path(
        &self,
        base: &Path,
        ctoken: &str,
        filter: impl Fn(&Path) -> bool,
    ) -> Vec<String> {
        trace!("complete_path: {ctoken}");
        let path = PathBuf::from(ctoken);
        let mut dir = path.parent().unwrap_or(&path).to_path_buf();
        if dir.is_relative() {
            dir = base.join(dir);
        }
        let mut prefix = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if path.is_dir() && ctoken.ends_with('/') {
            dir = path.to_path_buf();
            prefix = "".to_string();
        };
        std::fs::read_dir(dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|de| de.file_name().to_string_lossy().starts_with(&prefix))
            .map(|de| de.path())
            .filter(|p| filter(p))
            .map(|p| {
                p.strip_prefix(base)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .to_string()
            })
            .sorted()
            .collect()
    }

    fn generate_fig_script(&self, spec: &Spec) -> miette::Result<()> {
        let mut script = String::new();

        // ヘッダー情報を追加
        script.push_str("const completionSpec: Fig.Spec = {\n");
        script.push_str(&format!("  name: \"{}\",\n", spec.cmd.name));
        script.push_str(&format!(
            "  description: `{}`,\n",
            Self::escape_string(spec.cmd.help.as_deref().unwrap_or(""))
        ));

        // サブコマンドを追加
        script.push_str("  subcommands: [\n");
        for (_, subcmd) in &spec.cmd.subcommands {
            self.add_subcommand_to_script(&mut script, subcmd, 4)?;
        }
        script.push_str("  ],\n");

        // オプションを追加
        script.push_str("  options: [\n");
        for flag in &spec.cmd.flags {
            self.add_flag_to_script(&mut script, flag, 4)?;
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
        script.push_str(&format!(
            "{}{{
          name: \"{}\",
          description: `{}`,\n",
            indent_str,
            cmd.name,
            Self::escape_string(cmd.help.as_deref().unwrap_or(""))
        ));

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
            "{}{{
          name: [{}],
          description: `{}`,\n",
            indent_str,
            flag.short
                .iter()
                .map(|s| format!("\"-{}\"", s))
                .chain(flag.long.iter().map(|l| format!("\"--{}\"", l)))
                .collect::<Vec<_>>()
                .join(", "),
            Self::escape_string(flag.help.as_deref().unwrap_or(""))
        ));

        if flag.arg.is_some() {
            script.push_str(&format!("{}  args: {{\n", indent_str));
            // フラグの引数の詳細を追加
            script.push_str(&format!("{}  }},\n", indent_str));
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
            "{}{{
          name: \"{}\",
          description: `{}`,\n",
            indent_str,
            arg.name,
            Self::escape_string(arg.help.as_deref().unwrap_or(""))
        ));

        // 引数の追加情報（必須かどうかなど）を追加

        script.push_str(&format!("{}}},\n", indent_str));
        Ok(())
    }
    // 文字列をエスケープする関数
    fn escape_string(s: &str) -> String {
        s.replace('`', "\\`").replace('"', "\\\"")
    }
}

fn sh(script: &str) -> XXResult<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .env("__USAGE", env!("CARGO_PKG_VERSION"))
        .output()
        .map_err(|err| XXError::ProcessError(err, format!("sh -c {script}")))?;

    check_status(output.status)
        .map_err(|err| XXError::ProcessError(err, format!("sh -c {script}")))?;
    let stdout = String::from_utf8(output.stdout).expect("stdout is not utf-8");
    Ok(stdout)
}
