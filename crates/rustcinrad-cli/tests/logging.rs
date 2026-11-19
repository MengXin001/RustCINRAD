use anyhow::Result;
use std::process::Command;

#[test]
fn command_errors_are_structured_and_do_not_pollute_stdout() -> Result<()> {
    let directory = temp_dir::TempDir::new()?;
    let missing = directory.path().join("missing.bin");
    let output =
        Command::new(env!("CARGO_BIN_EXE_RustCINRAD")).arg("plot").arg(&missing).env("RUST_LOG", "info").output()?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("ERROR"));
    assert!(stderr.contains("command failed"));
    assert!(stderr.contains("error="));
    assert!(stderr.contains("missing.bin"));
    assert!(!stderr.contains("\u{1b}["));
    Ok(())
}

#[test]
fn rust_log_filters_diagnostics_without_changing_exit_status() -> Result<()> {
    let directory = temp_dir::TempDir::new()?;
    let output = Command::new(env!("CARGO_BIN_EXE_RustCINRAD"))
        .arg("plot")
        .arg(directory.path().join("missing.bin"))
        .env("RUST_LOG", "off")
        .output()?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    Ok(())
}

#[test]
fn help_documents_tilt_and_dtype_without_drawing_parameters() -> Result<()> {
    let output =
        Command::new(env!("CARGO_BIN_EXE_RustCINRAD")).args(["plot", "--help"]).env("RUST_LOG", "info").output()?;
    let stdout = String::from_utf8(output.stdout)?;
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(stdout.contains("--tilt"));
    assert!(stdout.contains("--dtype"));
    assert!(stdout.contains("[default: REF]"));
    for removed in ["--sweep", "--field", "--resolution-m", "--width", "--height <"] {
        assert!(!stdout.contains(removed));
    }
    Ok(())
}

#[test]
fn clap_help_and_argument_errors_keep_color_support() -> Result<()> {
    for (arguments, success, stderr) in
        [(vec!["plot", "--help"], true, false), (vec!["plot", "input.bin", "--dtype", "unknown"], false, true)]
    {
        let output = Command::new(env!("CARGO_BIN_EXE_RustCINRAD"))
            .args(arguments)
            .env("CLICOLOR_FORCE", "1")
            .env_remove("NO_COLOR")
            .env("RUST_LOG", "info")
            .output()?;
        assert_eq!(output.status.success(), success);
        let text = String::from_utf8(if stderr { output.stderr } else { output.stdout })?;
        assert!(text.contains("\u{1b}["), "expected ANSI styling: {text}");
    }
    Ok(())
}

#[test]
fn redirected_help_is_plain_text() -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_RustCINRAD"))
        .args(["plot", "--help"])
        .env("NO_COLOR", "1")
        .env_remove("CLICOLOR_FORCE")
        .output()?;
    assert!(output.status.success());
    assert!(!String::from_utf8(output.stdout)?.contains("\u{1b}["));
    Ok(())
}
