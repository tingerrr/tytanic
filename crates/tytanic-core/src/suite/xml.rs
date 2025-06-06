//! Implementation for emitting jUnit-XML of test results.

use std::ffi::OsStr;
use std::io::Write;

// TODO(tinger): Write errors and result types.
use chrono::Duration;
use typst::diag::SourceDiagnostic;
use xml::writer::Error as EmitterError;
use xml::writer::EventWriter;
use xml::writer::XmlEvent;
use xml::EmitterConfig;

use super::SuiteResult;
use super::TestResult;
use crate::diag;
use crate::doc::compare;
use crate::doc::compile;
use crate::test::Stage;
use crate::test::Test;

/// Formats a duration as a floating point literal
fn duration_to_float_repr(duration: Duration) -> String {
    let s = duration.num_seconds();
    let ms = duration.num_milliseconds() - (s * 1000);
    let float = s as f32 + ms as f32;

    float.to_string()
}

// TODO(tinger): we don't write the following attributes yet:
// - errors: these indicate unexpected failues, we don't treat these any
//           different, but could (for panics)

/// Write a jUnit-XML of the suite result file.
pub fn write_to_string(result: &SuiteResult) -> Result<String, EmitterError> {
    let mut w = EventWriter::new_with_config(
        vec![],
        EmitterConfig::new()
            .indent_string("    ")
            .perform_indent(true),
    );

    // NOTE(tinger): `testsuites` attributes and what they mean:
    // name       Name of the test suite (e.g. class name or folder name)
    // tests      Total number of tests in this suite
    // failures   Total number of failed tests in this suite
    // errors     Total number of errored tests in this suite
    // skipped    Total number of skipped tests in this suite
    // assertions Total number of assertions for all tests in this suite
    // time       Aggregated time of all tests in this file in seconds
    // timestamp  Date and time of when the test suite was executed (in ISO 8601
    //            format)

    let run_id = result.id.to_string();
    let tests = result.total().to_string();
    let failures = result.failed().to_string();
    let skipped = result.skipped().to_string();
    let duration = duration_to_float_repr(result.duration);

    w.write(
        XmlEvent::start_element("testsuite")
            .attr("name", &run_id)
            .attr("tests", &tests)
            .attr("failures", &failures)
            .attr("skipped", &skipped)
            .attr("time", &duration)
            .attr("timestamp", &result.timestamp.to_rfc3339()),
    )?;
    write_suite_result(&mut w, result)?;
    w.write(XmlEvent::end_element())?;

    Ok(String::from_utf8(w.into_inner()).expect("we only emit valid UTF-8"))
}

/// Writes a test suite result into the writer.
fn write_suite_result<W: Write>(
    w: &mut EventWriter<W>,
    result: &SuiteResult,
) -> Result<(), EmitterError> {
    // NOTE(tinger): `testsuite` attributes and what they mean:
    // name       Name of the test suite (e.g. class name or folder name)
    // tests      Total number of tests in this suite
    // failures   Total number of failed tests in this suite
    // errors     Total number of errored tests in this suite
    // skipped    Total number of skipped tests in this suite
    // assertions Total number of assertions for all tests in this suite
    // time       Aggregated time of all tests in this file in seconds
    // timestamp  Date and time of when the test suite was executed (in ISO 8601
    //            format)
    // file       Source code file of this test suite

    let run_id = result.id.to_string();
    let duration = duration_to_float_repr(result.duration);

    w.write(
        XmlEvent::start_element("testsuite")
            .attr("name", &run_id)
            .attr("tests", &result.total().to_string())
            .attr("failures", &result.failed().to_string())
            .attr("skipped", &result.skipped().to_string())
            .attr("time", &duration)
            .attr("timestamp", &result.timestamp.to_rfc3339()),
    )?;

    // write t4tr specific information about the run as custom properties
    w.write(XmlEvent::start_element("properties"))?;
    w.write(
        XmlEvent::start_element("property")
            .attr("name", "run-ID")
            .attr("value", &run_id),
    )?;
    w.write(XmlEvent::end_element())?;
    w.write(
        XmlEvent::start_element("property")
            .attr("name", "test-runner")
            .attr("value", "tytanic"),
    )?;
    w.write(XmlEvent::end_element())?;
    w.write(XmlEvent::end_element())?;

    for result in result.results.values() {
        write_test_result(w, &run_id, todo!(), result)?;
    }

    w.write(XmlEvent::end_element())?;

    Ok(())
}

/// Writes a single test result into the writer.
fn write_test_result<W: Write>(
    w: &mut EventWriter<W>,
    suite: &str,
    test: &Test,
    result: &TestResult,
) -> Result<(), EmitterError> {
    // NOTE(tinger): `testcase` attributes and what they mean:
    // name        The name of this test case, often the method name
    // classname   The name of the parent class/folder, often the same as the
    //             suite's name
    // assertions  Number of assertions checked during test case execution
    // time        Execution time of the test in seconds
    // file        Source code file of this test case
    // line        Source code line number of the start of this test case

    let time = duration_to_float_repr(result.duration());

    // TODO: write line attr from diagnostics

    w.write(
        XmlEvent::start_element("testcase")
            .attr("name", test.id().as_str())
            .attr("classname", suite)
            .attr("time", &time)
            .attr(
                "file",
                &test.id().to_path().join("test.typ").to_string_lossy(),
            ),
    )?;

    match result.stage() {
        Stage::Skipped => write_test_skip(w)?,
        Stage::Filtered => write_test_filter(w)?,
        Stage::FailedCompilation { error, reference } => {
            write_test_fail_compile(w, result.warnings(), error, *reference)?
        }
        Stage::FailedComparison(error) => write_test_fail_compare(w, result.warnings(), error)?,
        Stage::PassedCompilation => write_test_pass_compile(w, result.warnings())?,
        Stage::PassedComparison => write_test_pass_compare(w, result.warnings())?,
        Stage::Updated { optimized } => write_test_updated(w, result.warnings(), *optimized)?,
    }

    w.write(XmlEvent::end_element())?;

    Ok(())
}

fn write_test_fail_compile<W: Write>(
    w: &mut EventWriter<W>,
    warnings: &[SourceDiagnostic],
    result: &compile::Error,
    reference: bool,
) -> Result<(), EmitterError> {
    w.write(XmlEvent::start_element("failure").attr(
        "message",
        if reference {
            "Reference compilation failed"
        } else {
            "Compilation failed"
        },
    ))?;
    w.write(XmlEvent::end_element())?;
    write_test_diagnositcs(w, warnings, &result.0)?;
    Ok(())
}

fn write_test_fail_compare<W: Write>(
    w: &mut EventWriter<W>,
    warnings: &[SourceDiagnostic],
    result: &compare::Error,
) -> Result<(), EmitterError> {
    w.write(XmlEvent::start_element("failure").attr("message", "Comparison failed"))?;
    w.write(XmlEvent::end_element())?;
    write_test_diagnositcs(w, warnings, &[])?;
    Ok(())
}

fn write_test_pass_compile<W: Write>(
    w: &mut EventWriter<W>,
    warnings: &[SourceDiagnostic],
) -> Result<(), EmitterError> {
    write_test_diagnositcs(w, warnings, &[])?;
    Ok(())
}

fn write_test_pass_compare<W: Write>(
    w: &mut EventWriter<W>,
    warnings: &[SourceDiagnostic],
) -> Result<(), EmitterError> {
    write_test_diagnositcs(w, warnings, &[])?;
    Ok(())
}

fn write_test_updated<W: Write>(
    w: &mut EventWriter<W>,
    warnings: &[SourceDiagnostic],
    optimized: bool,
) -> Result<(), EmitterError> {
    write_test_diagnositcs(w, warnings, &[])?;
    todo!()
}

fn write_test_skip<W: Write>(w: &mut EventWriter<W>) -> Result<(), EmitterError> {
    w.write(XmlEvent::start_element("skipped").attr("message", "Test was skipped."))?;
    w.write(XmlEvent::end_element())?;
    Ok(())
}

fn write_test_filter<W: Write>(w: &mut EventWriter<W>) -> Result<(), EmitterError> {
    w.write(XmlEvent::start_element("skipped").attr("message", "Test was filtered out."))?;
    w.write(XmlEvent::end_element())?;

    Ok(())
}

/// Writes a single test result into the writer.
fn write_test_diagnositcs<W: Write>(
    w: &mut EventWriter<W>,
    warnings: &[SourceDiagnostic],
    errors: &[SourceDiagnostic],
) -> Result<(), EmitterError> {
    if !warnings.is_empty() || !errors.is_empty() {
        // NOTE(tinger): There is also system-out, but this isn't used for anything.
        w.write(XmlEvent::start_element("system-err"))?;

        let mut buf = vec![];
        let mut diags = termcolor::NoColor::new(&mut buf);

        // TODO(tinger): Emit tracing when this fails + write it into the system-err.
        // TODO(tinger): Pass down some diagnostic formatter for this.
        _ = diag::write_diagnostics(&mut diags, diagnostic_config, world, root, warnings, errors);

        w.write(XmlEvent::characters(&String::from_utf8_lossy(&buf)))?;
        w.write(XmlEvent::end_element())?;
    }

    Ok(())
}
