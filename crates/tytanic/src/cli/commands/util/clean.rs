use std::io::Write;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_utils::fmt::Term;

use super::Context;
use crate::cwrite;

pub fn run(ctx: &mut Context) -> eyre::Result<()> {
    let project = ctx.project()?;
    let suite = ctx.collect_tests(&project)?;

    let mut len = 0;
    for test in suite.unit_tests() {
        test.delete_temporary_directories(&project)?;
        len += 1;
    }

    let mut w = ctx.ui.stderr();
    write!(w, "Removed temporary directories for ")?;
    cwrite!(colored(w, Color::Green), "{len}")?;
    writeln!(w, " {}", Term::simple("test").with(len))?;

    Ok(())
}
