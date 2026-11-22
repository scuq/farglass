//! The release files: download, verification, and extraction (F067, ADR-048).
//!
//! A release has SHA256SUMS with a cosign keyless bundle. farglassctl runs the
//! static cosign binary of the sigstore project, which it downloads and accepts
//! only with the SHA-256 below. cosign checks that the release workflow of the
//! farglass repository signed SHA256SUMS at a v tag. farglassctl then checks the
//! SHA-256 of each file against SHA256SUMS. Each failure stops the install (ADR-042).

use std::fs;
use std::io::{self, Read as _};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

/// The repository of the releases.
pub const REPO: &str = "scuq/farglass";

/// The version of cosign that farglassctl runs.
pub const COSIGN_VERSION: &str = "v3.1.4";

/// The SHA-256 of the static cosign binary for each architecture, from the cosign release.
const COSIGN_SHA256: [(&str, &str); 2] = [
    (
        "amd64",
        "424e2c97d0b45fd7a6b608a87f090f35633f4c23ece5f092d48bba11aaf4bbfa",
    ),
    (
        "arm64",
        "ade83b7aac62e131cacfe96d958948b2150cc9efec46a570a2649b8e27dedad2",
    ),
];

/// The OIDC issuer of the GitHub Actions tokens.
const ISSUER: &str = "https://token.actions.githubusercontent.com";

/// The directory where farglassctl keeps cosign.
pub const TOOLS_DIR: &str = "/var/lib/farglassctl";

/// Returns the architecture name of the release files for this machine.
///
/// # Errors
///
/// A message if the architecture is not amd64 or arm64.
pub fn arch() -> Result<&'static str, String> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("arm64"),
        other => Err(format!(
            "The architecture {other} is not supported: farglass has amd64 and arm64."
        )),
    }
}

/// Returns the identity that must have signed SHA256SUMS: the release workflow at a v tag.
#[must_use]
pub fn identity_regexp() -> String {
    format!(
        "^https://github\\.com/{}/\\.github/workflows/release\\.yml@refs/tags/v",
        REPO.replace('.', "\\.")
    )
}

/// Returns the hexadecimal SHA-256 of `bytes`.
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Returns the hash of `name` in the text of a SHA256SUMS file.
#[must_use]
pub fn sum_of<'a>(sums: &'a str, name: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then_some(hash)
    })
}

/// Downloads `url` into memory, with at most `limit` bytes.
fn download(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let agent = ureq::Agent::config_builder()
        .user_agent(format!("farglassctl/{}", crate::version()))
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| format!("{url}: {error}"))?;
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|error| format!("{url}: {error}"))
}

/// Returns the tag of the newest release on GitHub.
///
/// # Errors
///
/// A message if GitHub does not answer.
pub fn latest_tag() -> Result<String, String> {
    let body = download(
        &format!("https://api.github.com/repos/{REPO}/releases/latest"),
        1 << 20,
    )?;
    let json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| format!("the answer of GitHub is not valid: {error}"))?;
    json["tag_name"]
        .as_str()
        .filter(|tag| tag.starts_with('v'))
        .map(str::to_owned)
        .ok_or_else(|| "GitHub gave no release tag".to_owned())
}

/// Downloads the files of the release `tag` for this architecture into `dir`.
///
/// # Errors
///
/// A message if a download fails.
pub fn fetch(tag: &str, dir: &Path) -> Result<(), String> {
    let arch = arch()?;
    for name in [
        "SHA256SUMS".to_owned(),
        "SHA256SUMS.sigstore.json".to_owned(),
        tarball_name(tag, arch),
        ctl_name(arch),
    ] {
        println!("Downloading {name}");
        let url = format!("https://github.com/{REPO}/releases/download/{tag}/{name}");
        let bytes = download(&url, 512 << 20)?;
        fs::write(dir.join(&name), bytes).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Returns the name of the tarball of `tag` for `arch`.
#[must_use]
pub fn tarball_name(tag: &str, arch: &str) -> String {
    format!("farglass-{tag}-linux-{arch}.tar.gz")
}

/// Returns the name of the static farglassctl of a release for `arch`.
#[must_use]
pub fn ctl_name(arch: &str) -> String {
    format!("farglassctl-linux-{arch}")
}

/// Returns the farglassctl of the release in `dir`, or `None` if `dir` has none.
/// Call it after [`verify`]: it checks only the SHA-256 against SHA256SUMS.
///
/// # Errors
///
/// A message if SHA256SUMS does not list the file, or the file has a different SHA-256.
pub fn find_ctl(dir: &Path) -> Result<Option<PathBuf>, String> {
    let path = dir.join(ctl_name(arch()?));
    if !path.exists() {
        return Ok(None);
    }
    let sums_path = dir.join("SHA256SUMS");
    let sums = fs::read_to_string(&sums_path)
        .map_err(|error| format!("{}: {error}", sums_path.display()))?;
    check_sum(&sums, &path)?;
    Ok(Some(path))
}

/// Checks the SHA-256 of the file at `path` against the text of SHA256SUMS.
fn check_sum(sums: &str, path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the file has no name")?;
    let expected = sum_of(sums, name)
        .ok_or_else(|| format!("SHA256SUMS does not list {name}. Nothing is installed."))?;
    let actual = sha256(&fs::read(path).map_err(|error| error.to_string())?);
    if actual != expected {
        return Err(format!(
            "{name} has the SHA-256 {actual}, not {expected}. Nothing is installed."
        ));
    }
    Ok(())
}

/// Returns the tarball in `dir`: the one file `farglass-*-linux-ARCH.tar.gz`.
///
/// # Errors
///
/// A message if there is no such file or more than one.
pub fn find_tarball(dir: &Path) -> Result<PathBuf, String> {
    let arch = arch()?;
    let suffix = format!("-linux-{arch}.tar.gz");
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("farglass-") && name.ends_with(&suffix))
        })
        .collect();
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(format!("{} has no farglass-*{suffix}", dir.display())),
        _ => Err(format!(
            "{} has more than one farglass-*{suffix}",
            dir.display()
        )),
    }
}

/// Returns the path of a cosign that farglassctl checked, and downloads it first if necessary.
///
/// # Errors
///
/// A message if the download fails or the binary has a different SHA-256.
pub fn cosign() -> Result<PathBuf, String> {
    let arch = arch()?;
    let expected = COSIGN_SHA256
        .iter()
        .find(|(name, _)| *name == arch)
        .map(|(_, hash)| *hash)
        .ok_or("no cosign for this architecture")?;
    let path = Path::new(TOOLS_DIR).join(format!("cosign-{COSIGN_VERSION}-{arch}"));
    // A file that is there must have the right hash too: a changed file is downloaded again.
    if let Ok(bytes) = fs::read(&path)
        && sha256(&bytes) == expected
    {
        return Ok(path);
    }
    println!("Downloading cosign {COSIGN_VERSION}");
    let url = format!(
        "https://github.com/sigstore/cosign/releases/download/{COSIGN_VERSION}/cosign-linux-{arch}"
    );
    let bytes = download(&url, 256 << 20)?;
    let actual = sha256(&bytes);
    if actual != expected {
        return Err(format!(
            "cosign has the SHA-256 {actual}, not {expected}. Nothing is installed."
        ));
    }
    fs::create_dir_all(TOOLS_DIR).map_err(|error| error.to_string())?;
    fs::set_permissions(TOOLS_DIR, fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;
    let temporary = path.with_extension("new");
    fs::write(&temporary, &bytes).map_err(|error| error.to_string())?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))
        .map_err(|error| error.to_string())?;
    fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Checks the release files in `dir`: the cosign signature of SHA256SUMS, then the hash of the tarball.
/// With `unsigned`, only the hash, for a test build with no signature.
///
/// # Errors
///
/// A message if a check fails. Nothing is installed then.
pub fn verify(dir: &Path, tarball: &Path, unsigned: bool) -> Result<(), String> {
    let sums_path = dir.join("SHA256SUMS");
    if unsigned {
        println!("WARNING: --allow-unsigned: the signature of the release is not checked.");
    } else {
        let cosign = cosign()?;
        let bundle = dir.join("SHA256SUMS.sigstore.json");
        if !bundle.exists() {
            return Err(format!(
                "{} is missing: the release has no signature.",
                bundle.display()
            ));
        }
        let status = std::process::Command::new(&cosign)
            .arg("verify-blob")
            .arg("--bundle")
            .arg(&bundle)
            .arg("--certificate-identity-regexp")
            .arg(identity_regexp())
            .arg("--certificate-oidc-issuer")
            .arg(ISSUER)
            .arg(&sums_path)
            .status()
            .map_err(|error| format!("cosign did not start: {error}"))?;
        if !status.success() {
            return Err(
                "The signature of SHA256SUMS is not valid. Nothing is installed.".to_owned(),
            );
        }
        println!("The signature of the release is valid.");
    }
    let sums = fs::read_to_string(&sums_path)
        .map_err(|error| format!("{}: {error}", sums_path.display()))?;
    check_sum(&sums, tarball)
}

/// Extracts the tarball into `dir` and returns its top directory.
///
/// # Errors
///
/// A message if the tarball is not valid or has a path outside its top directory.
pub fn extract(tarball: &Path, dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let file = fs::File::open(tarball).map_err(|error| error.to_string())?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    archive.set_preserve_permissions(true);
    let mut top: Option<PathBuf> = None;
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let path = entry
            .path()
            .map_err(|error| error.to_string())?
            .into_owned();
        let first = path
            .components()
            .next()
            .map(|part| PathBuf::from(part.as_os_str()));
        if top.is_none() {
            top.clone_from(&first);
        }
        if first != top {
            return Err(format!(
                "the tarball has a second top directory: {}",
                path.display()
            ));
        }
        // unpack_in refuses a path with .. or an absolute path.
        if !entry.unpack_in(dir).map_err(|error| error.to_string())? {
            return Err(format!(
                "the tarball has an unsafe path: {}",
                path.display()
            ));
        }
    }
    let top = top.ok_or("the tarball is empty")?;
    let root = dir.join(top);
    for required in [
        "bin/farglassd",
        "bin/farglass-login",
        "bin/farglass-session",
        "web/index.html",
        "VERSION",
    ] {
        if !root.join(required).exists() {
            return Err(format!("the tarball has no {required}"));
        }
    }
    Ok(root)
}

/// Reads the version of the extracted release at `root`.
///
/// # Errors
///
/// A message if the file is missing.
pub fn version_of(root: &Path) -> Result<String, String> {
    let mut text = String::new();
    fs::File::open(root.join("VERSION"))
        .and_then(|mut file| file.read_to_string(&mut text))
        .map_err(|error: io::Error| error.to_string())?;
    Ok(text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_names_only_the_release_workflow_at_a_v_tag() {
        assert_eq!(
            identity_regexp(),
            "^https://github\\.com/scuq/farglass/\\.github/workflows/release\\.yml@refs/tags/v"
        );
    }

    #[test]
    fn sha256sums_gives_the_hash_of_a_file() {
        let sums = "aaa  farglass-v1-linux-arm64.tar.gz\nbbb *farglassctl-linux-arm64\n";
        assert_eq!(sum_of(sums, "farglass-v1-linux-arm64.tar.gz"), Some("aaa"));
        assert_eq!(sum_of(sums, "farglassctl-linux-arm64"), Some("bbb"));
        assert_eq!(sum_of(sums, "farglass-v1-linux-amd64.tar.gz"), None);
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Writes a tarball with the files `files` and returns its path.
    fn tarball(dir: &Path, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(tarball_name("v9.9.9", arch().unwrap()));
        let file = fs::File::create(&path).unwrap();
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            file,
            flate2::Compression::fast(),
        ));
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    const RELEASE: [(&str, &[u8]); 5] = [
        ("farglass-v9.9.9/bin/farglassd", b"d"),
        ("farglass-v9.9.9/bin/farglass-login", b"l"),
        ("farglass-v9.9.9/bin/farglass-session", b"s"),
        ("farglass-v9.9.9/web/index.html", b"<html>"),
        ("farglass-v9.9.9/VERSION", b"v9.9.9\n"),
    ];

    #[test]
    fn a_release_with_the_right_hash_extracts_and_a_changed_one_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = tarball(dir.path(), &RELEASE);
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        assert_eq!(find_tarball(dir.path()).unwrap(), path);
        let hash = sha256(&fs::read(&path).unwrap());
        fs::write(dir.path().join("SHA256SUMS"), format!("{hash}  {name}\n")).unwrap();
        verify(dir.path(), &path, true).unwrap();
        let out = tempfile::tempdir().unwrap();
        let root = extract(&path, out.path()).unwrap();
        assert_eq!(version_of(&root).unwrap(), "v9.9.9");

        fs::write(
            dir.path().join("SHA256SUMS"),
            format!("{}  {name}\n", "0".repeat(64)),
        )
        .unwrap();
        assert!(
            verify(dir.path(), &path, true)
                .unwrap_err()
                .contains("Nothing is installed")
        );
        fs::write(dir.path().join("SHA256SUMS"), "").unwrap();
        assert!(
            verify(dir.path(), &path, true)
                .unwrap_err()
                .contains("does not list")
        );
    }

    #[test]
    fn the_farglassctl_of_a_release_needs_its_hash_in_sha256sums() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_ctl(dir.path()).unwrap(), None);
        let name = ctl_name(arch().unwrap());
        let path = dir.path().join(&name);
        fs::write(&path, b"new farglassctl").unwrap();
        let hash = sha256(b"new farglassctl");
        fs::write(dir.path().join("SHA256SUMS"), format!("{hash}  {name}\n")).unwrap();
        assert_eq!(find_ctl(dir.path()).unwrap(), Some(path.clone()));
        fs::write(&path, b"changed").unwrap();
        assert!(
            find_ctl(dir.path())
                .unwrap_err()
                .contains("Nothing is installed")
        );
        fs::write(dir.path().join("SHA256SUMS"), "").unwrap();
        assert!(find_ctl(dir.path()).unwrap_err().contains("does not list"));
    }

    #[test]
    fn a_tarball_with_a_path_outside_or_missing_files_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = tarball(dir.path(), &[("farglass-v9.9.9/VERSION", b"v9.9.9")]);
        let out = tempfile::tempdir().unwrap();
        assert!(
            extract(&path, out.path())
                .unwrap_err()
                .contains("has no bin/farglassd")
        );
        let dir = tempfile::tempdir().unwrap();
        let path = tarball(
            dir.path(),
            &[("farglass-v9.9.9/VERSION", b"v"), ("other/x", b"x")],
        );
        assert!(
            extract(&path, out.path())
                .unwrap_err()
                .contains("second top directory")
        );
    }
}
