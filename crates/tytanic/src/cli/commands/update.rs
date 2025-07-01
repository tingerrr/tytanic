use std::io::Write;

use color_eyre::eyre;
use tytanic_core::doc::compare::Strategy;
use tytanic_core::doc::render;
use tytanic_core::doc::render::Origin;
use tytanic_core::dsl;
use tytanic_core::suite::Filter;
use tytanic_core::Id;
use tytanic_filter::eval;

use super::CompareOptions;
use super::CompileOptions;
use super::Context;
use super::Direction;
use super::ExportOptions;
use super::FilterOptions;
use super::OptionDelegate;
use super::RunnerOptions;
use super::Switch;
use crate::cli::OperationFailure;
use crate::cli::TestFailure;
use crate::cli::CANCELLED;
use crate::report::Reporter;
use crate::runner::Action;
use crate::runner::Runner;
use crate::runner::RunnerConfig;
use crate::ui;

#[derive(clap::Args, Debug, Clone)]
#[group(id = "update-args")]
pub struct Args {
    #[command(flatten)]
    pub compile: CompileOptions,

    #[command(flatten)]
    pub compare: CompareOptions,

    #[command(flatten)]
    pub export: ExportOptions,

    #[command(flatten)]
    pub runner: RunnerOptions,

    #[command(flatten)]
    pub filter: FilterOptions,

    /// Update all included tests, even if they didn't fail.
    #[arg(long)]
    pub force: bool,
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

    let mut illegal_tests = vec![];
    for test in suite.matched() {
        if !test
            .as_unit_test()
            .is_some_and(|t| t.kind().is_persistent())
        {
            illegal_tests.push(test);
        }
    }

    if !illegal_tests.is_empty() {
        let mut w = ctx.ui.error()?;
        writeln!(w, "Cannot update tests:")?;
        for test in illegal_tests {
            ui::write_test_id(&mut w, test.id())?;
            writeln!(w)?;
        }
        eyre::bail!(OperationFailure);
    }

    let providers = ctx.providers(&project, &ctx.args.package, &ctx.args.font, &args.compile)?;

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

    let max_delta = args
        .compare
        .max_delta
        .unwrap_or(project.config().defaults.max_delta);

    let max_deviation = args
        .compare
        .max_deviations
        .unwrap_or(project.config().defaults.max_deviations);

    let runner = Runner::new(
        &project,
        &suite,
        &providers,
        RunnerConfig {
            warnings: args.compile.warnings.into_native(),
            optimize: args.export.optimize_refs.get_or_default(),
            fail_fast: args.runner.fail_fast.get_or_default(),
            pixel_per_pt,
            strategy: args
                .compare
                .compare
                .get_or_default()
                .then_some(Strategy::Simple {
                    max_delta,
                    max_deviation,
                }),
            export_ephemeral: args.export.export_ephemeral.get_or_default(),
            origin,
            action: Action::Update { force: args.force },
            cancellation: &CANCELLED,
        },
    );

    let reporter = Reporter::new(
        ctx.ui,
        &providers,
        ctx.ui.can_live_report() && ctx.args.output.verbose == 0,
    );
    let result = runner.run(&reporter)?;

    if !result.is_complete_pass() {
        eyre::bail!(TestFailure);
    }

    Ok(())
}
