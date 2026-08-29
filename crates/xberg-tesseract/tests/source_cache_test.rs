#[path = "../build_support/source_cache.rs"]
mod source_cache;

use source_cache::{SourceArtifact, copy_verified_artifact, prepare_source_tree, prepare_verified_artifact};
use std::cell::Cell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ARCHIVE_BYTES: &[u8] = b"verified archive bytes";
const ARCHIVE_SHA256: &str = "74e79ddd5e5b690dbfb0995c1af7c4a86feaeff0b5f5b8d667fc2cda3360a489";
const ALTERED_ARCHIVE_BYTES: &[u8] = b"altered archive bytes";
const MODEL_BYTES: &[u8] = b"verified model bytes";
const MODEL_SHA256: &str = "03cfa25d83f5eaa1faac98ed6ceaaf0e7afe3c273a1e1502c2714ebe10b8263e";
const ALTERED_MODEL_BYTES: &[u8] = b"altered model bytes";
const BUILD_SCRIPT: &str = include_str!("../build.rs");

const PINNED_BUILD_INPUTS: &[&str] = &[
    "13275a278eb55b5746e33f95fbf5a2c8f604b3ab",
    "0febcd4fc5cdc9c52d59509b45483d107f9f40922899e3f134ea615094ecbc77",
    "db0ec62f81b0737fbbe184d8fea40af5738f8eef",
    "d2470cc33ee34deeae6fc47809d0b33a3623a4343d92ff317ac3b9903c507bad",
    "87416418657359cb625c412a48b6e1d6d41c29bd",
    "7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2",
];

#[test]
fn should_store_verified_download_in_digest_addressed_cache() {
    let temp_dir = TestDir::new();
    let artifact = source_artifact();

    let prepared = prepare_verified_artifact(temp_dir.path(), &artifact, |download_path| {
        fs::write(download_path, ARCHIVE_BYTES)
    })
    .expect("prepare verified archive");

    assert_eq!(
        prepared.path,
        temp_dir
            .path()
            .join("native-sources")
            .join(ARCHIVE_SHA256)
            .join("tesseract.zip")
    );
    assert_eq!(prepared.bytes, ARCHIVE_BYTES);
    assert_eq!(fs::read(&prepared.path).expect("read cached archive"), ARCHIVE_BYTES);
    assert!(prepared.downloaded, "new archive should be reported as downloaded");
}

#[test]
fn should_wire_every_pinned_build_input_through_verification() {
    validate_build_source_contract(BUILD_SCRIPT).expect("build source contract must hold");
}

#[test]
fn should_reject_build_contract_when_digest_is_removed() {
    let mutated = BUILD_SCRIPT.replace(PINNED_BUILD_INPUTS[1], "");

    let error = validate_build_source_contract(&mutated).expect_err("missing digest must break build contract");

    assert_eq!(error, "missing pinned build input");
}

#[test]
fn should_reject_build_contract_when_model_uses_mutable_branch() {
    let mutated = BUILD_SCRIPT.replace(PINNED_BUILD_INPUTS[4], "main");

    let error = validate_build_source_contract(&mutated).expect_err("mutable model branch must break build contract");

    assert_eq!(error, "missing pinned build input");
}

#[test]
fn should_reuse_verified_content_addressed_cache_entry_without_fetching() {
    let temp_dir = TestDir::new();
    let artifact = source_artifact();
    prepare_verified_artifact(temp_dir.path(), &artifact, |download_path| {
        fs::write(download_path, ARCHIVE_BYTES)
    })
    .expect("seed verified archive");
    let fetch_called = Cell::new(false);

    let prepared = prepare_verified_artifact(temp_dir.path(), &artifact, |_| {
        fetch_called.set(true);
        Ok(())
    })
    .expect("reuse verified archive");

    assert!(!fetch_called.get(), "verified cache reuse must not fetch");
    assert!(
        !prepared.downloaded,
        "reused archive must not be reported as downloaded"
    );
    assert_eq!(prepared.bytes, ARCHIVE_BYTES);
}

#[test]
fn should_reject_download_when_archive_bytes_do_not_match_digest() {
    let temp_dir = TestDir::new();
    let artifact = source_artifact();

    let error = prepare_verified_artifact(temp_dir.path(), &artifact, |download_path| {
        fs::write(download_path, ALTERED_ARCHIVE_BYTES)
    })
    .expect_err("altered archive must be rejected");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!expected_cache_path(temp_dir.path()).exists());
}

#[test]
fn should_reject_artifact_when_digest_is_missing_before_fetching() {
    let temp_dir = TestDir::new();
    let artifact = SourceArtifact {
        name: "tesseract.zip",
        cache_key: "native-sources",
        sha256: "",
    };
    let fetch_called = Cell::new(false);

    let error = prepare_verified_artifact(temp_dir.path(), &artifact, |_| {
        fetch_called.set(true);
        Ok(())
    })
    .expect_err("missing digest must be rejected");

    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!fetch_called.get(), "missing digest must fail before fetch");
}

#[test]
fn should_reject_corrupt_cache_entry_without_fetching() {
    let temp_dir = TestDir::new();
    let artifact = source_artifact();
    let prepared = prepare_verified_artifact(temp_dir.path(), &artifact, |download_path| {
        fs::write(download_path, ARCHIVE_BYTES)
    })
    .expect("seed verified archive");
    fs::write(&prepared.path, ALTERED_ARCHIVE_BYTES).expect("corrupt cached archive");
    let fetch_called = Cell::new(false);

    let error = prepare_verified_artifact(temp_dir.path(), &artifact, |_| {
        fetch_called.set(true);
        Ok(())
    })
    .expect_err("corrupt cache entry must fail closed");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!fetch_called.get(), "corrupt cache must not trigger replacement fetch");
}

#[cfg(unix)]
#[test]
fn should_reject_symlinked_cache_component() {
    use std::os::unix::fs::symlink;

    let temp_dir = TestDir::new();
    let outside_dir = temp_dir.path().join("outside");
    fs::create_dir(&outside_dir).expect("create outside directory");
    symlink(&outside_dir, temp_dir.path().join("native-sources")).expect("create cache symlink");
    let fetch_called = Cell::new(false);

    let error = prepare_verified_artifact(temp_dir.path(), &source_artifact(), |_| {
        fetch_called.set(true);
        Ok(())
    })
    .expect_err("symlinked cache component must fail closed");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!fetch_called.get(), "symlink rejection must precede fetch");
    assert_eq!(fs::read_dir(&outside_dir).expect("read outside directory").count(), 0);
}

#[cfg(unix)]
#[test]
fn should_reject_cache_path_beneath_symlinked_root() {
    use std::os::unix::fs::symlink;

    let temp_dir = TestDir::new();
    let outside_dir = temp_dir.path().join("outside-root");
    fs::create_dir(&outside_dir).expect("create outside directory");
    let outside_artifacts = outside_dir.join("source-artifacts");
    fs::create_dir(&outside_artifacts).expect("create pre-existing outside artifact directory");
    let cache_root = temp_dir.path().join("cache-root");
    symlink(&outside_dir, &cache_root).expect("create cache-root symlink");
    let fetch_called = Cell::new(false);

    let error = prepare_verified_artifact(&cache_root.join("source-artifacts"), &source_artifact(), |_| {
        fetch_called.set(true);
        Ok(())
    })
    .expect_err("cache path beneath a symlinked root must fail closed");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(!fetch_called.get(), "symlink rejection must precede fetch");
    assert_eq!(
        fs::read_dir(&outside_artifacts)
            .expect("read outside directory")
            .count(),
        0
    );
}

#[cfg(unix)]
#[test]
fn should_reject_symlinked_source_root_without_removing_target() {
    use std::os::unix::fs::symlink;

    let temp_dir = TestDir::new();
    let archive = prepare_verified_artifact(temp_dir.path(), &source_artifact(), |download_path| {
        fs::write(download_path, ARCHIVE_BYTES)
    })
    .expect("prepare verified archive");
    let outside_dir = temp_dir.path().join("outside-source");
    fs::create_dir(&outside_dir).expect("create outside source directory");
    fs::write(outside_dir.join("sentinel"), "preserve").expect("write outside sentinel");
    let third_party_dir = temp_dir.path().join("third-party");
    symlink(&outside_dir, &third_party_dir).expect("create source-root symlink");

    let error = prepare_source_tree(&third_party_dir, "tesseract", &archive, |_, _| Ok(()))
        .expect_err("symlinked source root must fail closed");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(
        outside_dir.join("sentinel").is_file(),
        "outside target must remain intact"
    );
}

#[test]
fn should_replace_poisoned_complete_source_tree_before_extraction() {
    let temp_dir = TestDir::new();
    let artifact = prepare_verified_artifact(temp_dir.path(), &source_artifact(), |download_path| {
        fs::write(download_path, ARCHIVE_BYTES)
    })
    .expect("prepare verified archive");
    let third_party_dir = temp_dir.path().join("third-party");
    let source_dir = third_party_dir.join("tesseract");
    fs::create_dir_all(&source_dir).expect("create poisoned source tree");
    fs::write(source_dir.join("CMakeLists.txt"), "poisoned build").expect("write poisoned marker");
    fs::write(source_dir.join("poisoned.cpp"), "malicious source").expect("write poisoned source");
    let extraction_called = Cell::new(false);

    let prepared = prepare_source_tree(&third_party_dir, "tesseract", &artifact, |bytes, destination| {
        extraction_called.set(true);
        assert_eq!(bytes, ARCHIVE_BYTES);
        assert!(!destination.exists(), "poisoned tree must be removed before extraction");
        fs::create_dir_all(destination)?;
        fs::write(destination.join("CMakeLists.txt"), "verified build")?;
        Ok(())
    })
    .expect("reconstruct source tree from verified archive");

    assert!(
        extraction_called.get(),
        "complete-looking source tree must not be reused"
    );
    assert_eq!(
        fs::read_to_string(prepared.path.join("CMakeLists.txt")).expect("read verified marker"),
        "verified build"
    );
    assert!(!prepared.path.join("poisoned.cpp").exists());
}

#[test]
fn should_fail_before_source_preparation_when_archive_is_altered() {
    let temp_dir = TestDir::new();
    let artifact = source_artifact();
    let extraction_called = Cell::new(false);

    let error = prepare_verified_artifact(temp_dir.path(), &artifact, |download_path| {
        fs::write(download_path, ALTERED_ARCHIVE_BYTES)
    })
    .and_then(|verified| {
        prepare_source_tree(&temp_dir.path().join("third-party"), "tesseract", &verified, |_, _| {
            extraction_called.set(true);
            Ok(())
        })
    })
    .expect_err("altered archive must fail verification");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(
        !extraction_called.get(),
        "verification failure must precede source preparation"
    );
}

#[test]
fn should_reject_altered_existing_model_file() {
    let temp_dir = TestDir::new();
    let model = SourceArtifact {
        name: "eng.traineddata",
        cache_key: "tessdata",
        sha256: MODEL_SHA256,
    };
    let verified = prepare_verified_artifact(temp_dir.path(), &model, |download_path| {
        fs::write(download_path, MODEL_BYTES)
    })
    .expect("prepare verified model");
    let destination = temp_dir.path().join("out").join("eng.traineddata");
    fs::create_dir_all(destination.parent().expect("model parent")).expect("create model output directory");
    fs::write(&destination, ALTERED_MODEL_BYTES).expect("write altered installed model");

    let error = copy_verified_artifact(&verified, &destination).expect_err("altered model must fail closed");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(fs::read(&destination).expect("read altered model"), ALTERED_MODEL_BYTES);
}

fn source_artifact() -> SourceArtifact<'static> {
    SourceArtifact {
        name: "tesseract.zip",
        cache_key: "native-sources",
        sha256: ARCHIVE_SHA256,
    }
}

fn expected_cache_path(root: &Path) -> PathBuf {
    root.join("native-sources").join(ARCHIVE_SHA256).join("tesseract.zip")
}

fn validate_build_source_contract(build_script: &str) -> Result<(), &'static str> {
    if PINNED_BUILD_INPUTS.iter().any(|input| !build_script.contains(input)) {
        return Err("missing pinned build input");
    }
    if build_script.contains("refs/tags") || build_script.contains("tessdata_fast/main") {
        return Err("mutable build input URL");
    }
    if build_script.matches("&LEPTONICA_SOURCE").count() != 2
        || build_script.matches("&TESSERACT_SOURCE").count() != 2
        || build_script
            .matches("prepare_eng_traineddata(&artifact_cache_dir")
            .count()
            != 2
        || !build_script.contains("prepare_verified_artifact(artifact_cache_dir, &source.artifact")
        || !build_script.contains("prepare_verified_artifact(artifact_cache_dir, &ENG_TRAINEDDATA_ARTIFACT")
    {
        return Err("build input bypasses verification");
    }
    Ok(())
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "xberg-source-cache-test-{}-{:?}-{unique}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir(&path).expect("create temporary test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).expect("remove temporary test directory");
    }
}
