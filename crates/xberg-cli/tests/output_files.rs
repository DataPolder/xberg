use std::process::Command;

fn xberg_bin() -> &'static str {
    env!("CARGO_BIN_EXE_xberg")
}

fn html_input(dir: &tempfile::TempDir, name: &str, heading: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, format!("<h1>{heading}</h1><p>Converted body.</p>")).expect("write HTML input");
    path
}

#[test]
fn extract_output_writes_converted_content_without_printing_it() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let input = html_input(&dir, "source.html", "Converted heading");
    let destination = dir.path().join("converted.md");

    let output = Command::new(xberg_bin())
        .args([
            "extract",
            &input.to_string_lossy(),
            "--content-format",
            "markdown",
            "--output",
            &destination.to_string_lossy(),
        ])
        .output()
        .expect("run xberg extract");

    assert!(
        output.status.success(),
        "extract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"");
    assert_eq!(
        std::fs::read_to_string(destination).expect("read converted output"),
        "# Converted heading\n\nConverted body.\n"
    );
}

#[test]
fn extract_output_refuses_to_overwrite_an_existing_file() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let input = html_input(&dir, "source.html", "New heading");
    let destination = dir.path().join("converted.md");
    std::fs::write(&destination, "keep me").expect("seed destination");

    let output = Command::new(xberg_bin())
        .args([
            "extract",
            &input.to_string_lossy(),
            "--content-format",
            "markdown",
            "--output",
            &destination.to_string_lossy(),
        ])
        .output()
        .expect("run xberg extract");

    assert!(!output.status.success(), "extract overwrote an existing file");
    assert_eq!(std::fs::read_to_string(destination).unwrap(), "keep me");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already exists"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn extract_output_preserves_the_json_envelope_on_stdout() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let input = html_input(&dir, "source.html", "Converted heading");
    let destination = dir.path().join("converted.md");

    let output = Command::new(xberg_bin())
        .args([
            "extract",
            &input.to_string_lossy(),
            "--content-format",
            "markdown",
            "--format",
            "json",
            "--output",
            &destination.to_string_lossy(),
        ])
        .output()
        .expect("run xberg extract");

    assert!(
        output.status.success(),
        "extract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout must remain JSON");
    assert_eq!(
        envelope["result"]["content"],
        "# Converted heading\n\nConverted body.\n"
    );
    assert_eq!(
        std::fs::read_to_string(destination).unwrap(),
        "# Converted heading\n\nConverted body.\n"
    );
}

#[test]
fn batch_output_writes_one_converted_file_per_input() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let first = html_input(&dir, "first.html", "First heading");
    let second = html_input(&dir, "second.html", "Second heading");
    let destination = dir.path().join("converted");
    std::fs::create_dir(&destination).expect("create output directory");

    let output = Command::new(xberg_bin())
        .args([
            "batch",
            &first.to_string_lossy(),
            &second.to_string_lossy(),
            "--format",
            "text",
            "--content-format",
            "markdown",
            "--output",
            &destination.to_string_lossy(),
        ])
        .output()
        .expect("run xberg batch");

    assert!(
        output.status.success(),
        "batch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"");
    assert_eq!(
        std::fs::read_to_string(destination.join("first.md")).unwrap(),
        "# First heading\n\nConverted body.\n"
    );
    assert_eq!(
        std::fs::read_to_string(destination.join("second.md")).unwrap(),
        "# Second heading\n\nConverted body.\n"
    );
}

#[test]
fn batch_output_refuses_colliding_input_names_without_writing_files() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let first_dir = dir.path().join("one");
    let second_dir = dir.path().join("two");
    let destination = dir.path().join("converted");
    std::fs::create_dir(&first_dir).unwrap();
    std::fs::create_dir(&second_dir).unwrap();
    std::fs::create_dir(&destination).unwrap();
    let first = first_dir.join("same.html");
    let second = second_dir.join("same.html");
    std::fs::write(&first, "<p>First</p>").unwrap();
    std::fs::write(&second, "<p>Second</p>").unwrap();

    let output = Command::new(xberg_bin())
        .args([
            "batch",
            &first.to_string_lossy(),
            &second.to_string_lossy(),
            "--format",
            "text",
            "--content-format",
            "markdown",
            "--output",
            &destination.to_string_lossy(),
        ])
        .output()
        .expect("run xberg batch");

    assert!(!output.status.success(), "batch accepted colliding output names");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("same.md"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_dir(destination).unwrap().count(), 0);
}
