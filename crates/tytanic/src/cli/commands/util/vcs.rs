use std::io::Write;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_utils::fmt::Term;

use super::Context;
use crate::cli::OperationFailure;
use crate::cwrite;

#[derive(clap::Args, Debug, Clone)]
#[group(id = "util-vcs-args")]
pub struct Args {
    /// The sub command to run
    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Command {
    /// Rewrite all ignore files
    #[command()]
    Ignore,
}

impl Command {
    pub fn run(&self, ctx: &mut Context) -> eyre::Result<()> {
        match self {
            Command::Ignore => {
                let project = ctx.project()?;
                let Some(vcs) = project.vcs() else {
                    writeln!(ctx.ui.warn()?, "no VCS detected")?;
                    eyre::bail!(OperationFailure);
                };

                let suite = ctx.collect_tests(&project)?;

                let mut len = 0;
                for test in suite.unit_tests() {
                    vcs.ignore(&project, test)?;
                    len += 1;
                }

                let mut w = ctx.ui.stderr();
                write!(w, "Rewritten ignore files for ")?;
                cwrite!(colored(w, Color::Green), "{len}")?;
                writeln!(w, " {}", Term::simple("test").with(len))?;

                Ok(())
            }
        }
    }
}
