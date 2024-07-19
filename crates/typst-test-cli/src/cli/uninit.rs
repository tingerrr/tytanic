use std::io::Write;

use typst_test_lib::test_set::eval::AllMatcher;

use super::Context;
use crate::util;

pub fn run(ctx: &mut Context) -> anyhow::Result<()> {
    let mut project = ctx.ensure_project()?;
    project.collect_tests(AllMatcher)?;
    let count = project.matched().len();

    // TODO: confirmation?

    project.uninit()?;
    writeln!(
        ctx.reporter.lock().unwrap(),
        "Removed {} {}",
        count,
        util::fmt::plural(count, "test"),
    )?;

    Ok(())
}
