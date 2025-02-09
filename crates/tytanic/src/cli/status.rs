use std::io::Write;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_core::test::Kind;

use super::Context;
use crate::cwrite;
use crate::json::ProjectJson;

#[derive(clap::Args, Debug, Clone)]
#[group(id = "status-args")]
pub struct Args {
    /// Print a JSON describing the project to stdout
    #[arg(long)]
    pub json: bool,
}

pub fn run(ctx: &mut Context, args: &Args) -> eyre::Result<()> {
    let project = ctx.project()?;
    let paths = project.paths();
    let suite = ctx.collect_all_tests(&project)?;

    let delim_open = " ┌ ";
    let delim_middle = " ├ ";
    let delim_close = " └ ";

    let manifest = match project.read_manifest() {
        Ok(m) => m,
        Err(err) => {
            writeln!(ctx.ui.warn()?, "Couldn't parse manifest:\n{err}")?;
            None
        }
    };

    if args.json {
        serde_json::to_writer_pretty(
            ctx.ui.stdout(),
            &ProjectJson::new(&project, manifest.as_ref(), &suite),
        )?;
        return Ok(());
    }

    let mut w = ctx.ui.stderr();

    let align = ["Template", "Project", "Tests"]
        .map(str::len)
        .into_iter()
        .max()
        .unwrap();

    if let Some(package) = manifest.as_ref().map(|p| &p.package) {
        write!(w, "{:>align$}{}", "Project", delim_open)?;
        cwrite!(bold_colored(w, Color::Cyan), "{}", package.name)?;
        write!(w, ":")?;
        cwrite!(bold_colored(w, Color::Cyan), "{}", package.version)?;
    } else {
        write!(w, "{:>align$}{}", "Project", delim_open)?;
        cwrite!(bold_colored(w, Color::Yellow), "none")?;
    }
    writeln!(w)?;

    write!(w, "{:>align$}{}", "Vcs", delim_middle)?;
    if let Some(vcs) = project.vcs() {
        cwrite!(bold_colored(w, Color::Green), "{vcs}")?;
    } else {
        cwrite!(bold_colored(w, Color::Yellow), "none")?;
    }
    writeln!(w)?;

    write!(w, "{:>align$}{}", "Template", delim_middle)?;
    if suite.template().is_some() {
        let path = paths.template();
        let path = path
            .strip_prefix(paths.project_root())
            .expect("template is in project root");

        cwrite!(bold_colored(w, Color::Cyan), "{}", path.display())?;
    } else {
        cwrite!(bold_colored(w, Color::Green), "none")?;
    }
    writeln!(w)?;

    if suite.matched().is_empty() {
        write!(w, "{:>align$}{}", "Tests", delim_close)?;
        cwrite!(bold_colored(w, Color::Cyan), "none")?;
        writeln!(w)?;
    } else {
        let mut persistent = 0;
        let mut ephemeral = 0;
        let mut compile_only = 0;

        for test in suite.matched().values() {
            match test.kind() {
                Kind::Persistent => persistent += 1,
                Kind::Ephemeral => ephemeral += 1,
                Kind::CompileOnly => compile_only += 1,
            }
        }

        write!(w, "{:>align$}{}", "Tests", delim_middle)?;
        cwrite!(bold_colored(w, Color::Green), "{persistent}")?;
        writeln!(w, " persistent")?;

        write!(w, "{:>align$}{}", "", delim_middle)?;
        cwrite!(bold_colored(w, Color::Green), "{ephemeral}")?;
        writeln!(w, " ephemeral")?;

        write!(w, "{:>align$}{}", "", delim_close)?;
        cwrite!(bold_colored(w, Color::Yellow), "{compile_only}")?;
        writeln!(w, " compile-only")?;
    }

    Ok(())
}
