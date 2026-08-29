use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

const SOURCE_ROOT_MARKER: &str = "CMakeLists.txt";
const SHA256_HEX_LENGTH: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceArtifact<'a> {
    pub(crate) name: &'a str,
    pub(crate) cache_key: &'a str,
    pub(crate) sha256: &'a str,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct VerifiedArtifact {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) downloaded: bool,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PreparedSourceTree {
    pub(crate) path: PathBuf,
    pub(crate) downloaded: bool,
}

pub(crate) fn source_tree_is_complete(source_dir: &Path) -> bool {
    source_dir.is_dir() && source_dir.join(SOURCE_ROOT_MARKER).is_file()
}

pub(crate) fn prepare_verified_artifact(
    cache_root: &Path,
    artifact: &SourceArtifact<'_>,
    fetch: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<VerifiedArtifact> {
    validate_artifact(artifact)?;
    ensure_directory(cache_root)?;

    let cache_key_dir = cache_root.join(artifact.cache_key);
    ensure_directory(&cache_key_dir)?;
    let digest_dir = cache_key_dir.join(artifact.sha256);
    ensure_directory(&digest_dir)?;
    let artifact_path = digest_dir.join(artifact.name);
    if artifact_path.exists() {
        let bytes = read_verified_file(&artifact_path, artifact)?;
        return Ok(VerifiedArtifact {
            path: artifact_path,
            bytes,
            downloaded: false,
        });
    }

    let temporary_path = temporary_path(&artifact_path);
    remove_if_exists(&temporary_path)?;
    if let Err(error) = fetch(&temporary_path) {
        let _ = remove_if_exists(&temporary_path);
        return Err(error);
    }

    let bytes = match read_verified_file(&temporary_path, artifact) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = remove_if_exists(&temporary_path);
            return Err(error);
        }
    };

    match fs::rename(&temporary_path, &artifact_path) {
        Ok(()) => {}
        Err(_) if artifact_path.exists() => {
            remove_if_exists(&temporary_path)?;
            let bytes = read_verified_file(&artifact_path, artifact)?;
            return Ok(VerifiedArtifact {
                path: artifact_path,
                bytes,
                downloaded: false,
            });
        }
        Err(error) => {
            let _ = remove_if_exists(&temporary_path);
            return Err(error);
        }
    }

    Ok(VerifiedArtifact {
        path: artifact_path,
        bytes,
        downloaded: true,
    })
}

pub(crate) fn prepare_source_tree(
    third_party_dir: &Path,
    source_name: &str,
    archive: &VerifiedArtifact,
    extract: impl FnOnce(&[u8], &Path) -> io::Result<()>,
) -> io::Result<PreparedSourceTree> {
    validate_path_component(source_name, "source name")?;
    ensure_directory(third_party_dir)?;

    let source_dir = third_party_dir.join(source_name);
    let staging_dir = temporary_path(&source_dir);
    remove_if_exists(&staging_dir)?;
    if let Err(error) = extract(&archive.bytes, &staging_dir) {
        let _ = remove_if_exists(&staging_dir);
        return Err(error);
    }

    if !source_tree_is_complete(&staging_dir) {
        let _ = remove_if_exists(&staging_dir);
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "extracted {source_name} source is incomplete: missing {}",
                staging_dir.join(SOURCE_ROOT_MARKER).display()
            ),
        ));
    }

    remove_if_exists(&source_dir)?;
    if let Err(error) = fs::rename(&staging_dir, &source_dir) {
        let _ = remove_if_exists(&staging_dir);
        return Err(error);
    }

    Ok(PreparedSourceTree {
        path: source_dir,
        downloaded: archive.downloaded,
    })
}

pub(crate) fn copy_verified_artifact(artifact: &VerifiedArtifact, destination: &Path) -> io::Result<()> {
    let expected = sha256_hex(&artifact.bytes);
    if destination.exists() {
        return verify_bytes(&fs::read(destination)?, &expected, &destination.display().to_string());
    }

    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("artifact destination has no parent: {}", destination.display()),
        )
    })?;
    ensure_directory(parent)?;

    let temporary_path = temporary_path(destination);
    remove_if_exists(&temporary_path)?;
    fs::write(&temporary_path, &artifact.bytes)?;

    if let Err(error) = verify_bytes(
        &fs::read(&temporary_path)?,
        &expected,
        &temporary_path.display().to_string(),
    ) {
        let _ = remove_if_exists(&temporary_path);
        return Err(error);
    }

    match fs::rename(&temporary_path, destination) {
        Ok(()) => Ok(()),
        Err(_) if destination.exists() => {
            remove_if_exists(&temporary_path)?;
            verify_bytes(&fs::read(destination)?, &expected, &destination.display().to_string())
        }
        Err(error) => {
            let _ = remove_if_exists(&temporary_path);
            Err(error)
        }
    }
}

fn read_verified_file(path: &Path, artifact: &SourceArtifact<'_>) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cached artifact is not a regular file: {}", path.display()),
        ));
    }

    let bytes = fs::read(path)?;
    verify_bytes(&bytes, artifact.sha256, artifact.name)?;
    Ok(bytes)
}

fn verify_bytes(bytes: &[u8], expected_sha256: &str, label: &str) -> io::Result<()> {
    let actual_sha256 = sha256_hex(bytes);
    if actual_sha256 == expected_sha256 {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("SHA-256 mismatch for {label}: expected {expected_sha256}, got {actual_sha256}"),
    ))
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(SHA256_HEX_LENGTH);
    for byte in digest {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

fn validate_artifact(artifact: &SourceArtifact<'_>) -> io::Result<()> {
    validate_path_component(artifact.name, "artifact name")?;
    validate_path_component(artifact.cache_key, "artifact cache key")?;

    if artifact.sha256.len() != SHA256_HEX_LENGTH
        || !artifact
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid SHA-256 digest for {}", artifact.name),
        ));
    }

    Ok(())
}

fn validate_path_component(value: &str, label: &str) -> io::Result<()> {
    let valid = !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if valid {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("invalid {label}: {value}"),
    ))
}

fn temporary_path(destination: &Path) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    destination.with_file_name(format!(".{file_name}.{}.partial", std::process::id()))
}

pub(crate) fn ensure_directory(path: &Path) -> io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty() && *parent != path)
    {
        ensure_directory(parent)?;
    }

    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(metadata) if trusted_directory_symlink(path, &metadata) => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("cache path is not a regular directory: {}", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => match fs::create_dir(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => ensure_directory(path),
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn trusted_directory_symlink(path: &Path, metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
        && metadata.uid() == 0
        && fs::metadata(path).is_ok_and(|target| target.file_type().is_dir())
}

#[cfg(not(unix))]
fn trusted_directory_symlink(_path: &Path, _metadata: &fs::Metadata) -> bool {
    false
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}
