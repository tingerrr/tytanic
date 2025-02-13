use std::io::Write;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_core::suite::Filter;
use tytanic_core::test::Test;
use tytanic_core::{dsl, Id};
use tytanic_filter::eval;
use tytanic_utils::fmt::Term;

use super::{Context, FilterOptions};
use crate::cli::OperationFailure;
use crate::cwrite;

#[derive(clap::Args, Debug, Clone)]
#[group(id = "delete-args")]
pub struct Args {
    #[command(flatten)]
    pub filter: FilterOptions,
}

pub fn run(ctx: &mut Context, args: &Args) -> eyre::Result<()> {
    let project = ctx.project()?;

    let filter = match ctx.filter(&args.filter)? {
        Filter::TestSet(set) => {
            Filter::TestSet(set.map(|set| eval::Set::expr_diff(set, dsl::built_in::template())))
        }
        Filter::Explicit(explicit) => {
            if explicit.contains(&Id::template()) {
                writeln!(ctx.ui.error()?, "Cannot delete template test")?;
                eyre::bail!(OperationFailure);
            }

            Filter::Explicit(explicit)
        }
    };

    let suite = ctx.collect_tests_with_filter(&project, filter)?;

    if suite.matched().len() > 1 {
        if let Filter::TestSet(set) = suite.filter() {
            if !set.all() {
                ctx.error_too_many_tests(&args.filter.expression)?;
                eyre::bail!(OperationFailure);
            }
        }
    }

    for test in suite.matched() {
        if let Test::Unit(test) = test {
            test.delete(&project)?;
        }
    }

    let len = suite.matched().len();

    let mut w = ctx.ui.stderr();
    write!(w, "Deleted ")?;
    cwrite!(bold_colored(w, Color::Green), "{len}")?;
    writeln!(w, " {}", Term::simple("test").with(len))?;

    Ok(())
}
