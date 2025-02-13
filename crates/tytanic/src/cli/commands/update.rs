use std::io::Write;

use color_eyre::eyre;
use tytanic_core::doc::render::{self, Origin};
use tytanic_core::suite::Filter;
use tytanic_core::{dsl, Id};
use tytanic_filter::eval;

use super::{
    CompileOptions, Context, Direction, ExportOptions, FilterOptions, OptionDelegate,
    RunnerOptions, Switch,
};
use crate::cli::{OperationFailure, TestFailure, CANCELLED};
use crate::report::Reporter;
use crate::runner::{Action, Runner, RunnerConfig};

#[derive(clap::Args, Debug, Clone)]
#[group(id = "update-args")]
pub struct Args {
    #[command(flatten)]
    pub compile: CompileOptions,

    #[command(flatten)]
    pub export: ExportOptions,

    #[command(flatten)]
    pub runner: RunnerOptions,

    #[command(flatten)]
    pub filter: FilterOptions,
}

pub fn run(ctx: &mut Context, args: &Args) -> eyre::Result<()> {
    let project = ctx.project()?;
    let filter = match ctx.filter(&args.filter)? {
        Filter::TestSet(set) => Filter::TestSet(
            set.map(|set| eval::Set::expr_inter(set, dsl::built_in::persistent(), [])),
        ),
        Filter::Explicit(explicit) => {
            if explicit.contains(&Id::template()) {
                writeln!(ctx.ui.error()?, "Cannot update template test")?;
                eyre::bail!(OperationFailure);
            }

            Filter::Explicit(explicit)
        }
    };

    let suite = ctx.collect_tests_with_filter(&project, filter)?;
    let world = ctx.world(&args.compile)?;

    let origin = match args
        .export
        .dir
        .map(OptionDelegate::into_native)
        .unwrap_or(project.config().defaults.direction)
    {
        Direction::Ltr => Origin::TopLeft,
        Direction::Rtl => Origin::TopRight,
    };

    let pixel_per_pt = render::ppi_to_ppp(args.export.ppi.unwrap_or(project.config().defaults.ppi));

    let runner = Runner::new(
        &project,
        &suite,
        &world,
        RunnerConfig {
            warnings: args.compile.warnings.into_native(),
            optimize: args.export.optimize_refs.get_or_default(),
            fail_fast: args.runner.fail_fast.get_or_default(),
            pixel_per_pt,
            action: Action::Update {
                export_ephemeral: args.export.export_ephemeral.get_or_default(),
                origin,
            },
            cancellation: &CANCELLED,
        },
    );

    let reporter = Reporter::new(
        ctx.ui,
        &world,
        ctx.ui.can_live_report() && ctx.args.output.verbose == 0,
    );
    let result = runner.run(&reporter)?;

    if !result.is_complete_pass() {
        eyre::bail!(TestFailure);
    }

    Ok(())
}
