use std::io::Write;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_core::stdx::fmt::Term;

use super::Context;
use crate::ui;

pub fn run(ctx: &mut Context) -> eyre::Result<()> {
    let project = ctx.project()?;
    let suite = ctx.collect_all_tests(&project)?;

    let len = suite.matched().len();

    for test in suite.matched().values() {
        test.delete_temporary_directories(project.paths())?;
    }

    let mut w = ctx.ui.stderr();
    write!(w, "Removed temporary directories for ")?;
    ui::write_colored(&mut w, Color::Green, |w| write!(w, "{len}"))?;
    writeln!(w, " {}", Term::simple("test").with(len))?;

    Ok(())
}
