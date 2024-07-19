use super::{CliResult, Context, Global};
use crate::cli::{bail_if_invalid_matcher_expr, bail_if_uninit};

pub fn run(ctx: Context, global: &Global) -> anyhow::Result<CliResult> {
    bail_if_uninit!(ctx);

    bail_if_invalid_matcher_expr!(global => matcher);
    ctx.project.collect_tests(matcher)?;
    ctx.reporter.tests(ctx.project)?;

    Ok(CliResult::Ok)
}
