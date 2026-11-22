//! A plan of the changes to the host: a list of steps that farglassctl does in order,
//! or prints with `--dry-run` (F067).

use std::fmt;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One change to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Runs a program. A nonzero exit status stops the plan.
    Run(Vec<String>),
    /// Runs a program. A nonzero exit status is no error, for example a unit that is not there.
    TryRun(Vec<String>),
    /// Copies a file with the mode. The new file goes in place atomically.
    Copy {
        /// The source file.
        from: PathBuf,
        /// The target file.
        to: PathBuf,
        /// The permissions.
        mode: u32,
    },
    /// Replaces a directory with a copy of a different one.
    ReplaceDir {
        /// The source directory.
        from: PathBuf,
        /// The target directory.
        to: PathBuf,
    },
    /// Writes a file with the mode, atomically.
    Write {
        /// The target file.
        to: PathBuf,
        /// The content.
        content: String,
        /// The permissions.
        mode: u32,
    },
    /// Removes a file or a directory with its content. A missing path is no error.
    Remove(PathBuf),
    /// Prints a note for the administrator.
    Note(String),
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Run(args) => write!(f, "run: {}", args.join(" ")),
            Self::TryRun(args) => write!(f, "run (errors ignored): {}", args.join(" ")),
            Self::Copy { from, to, mode } => {
                write!(
                    f,
                    "copy {} to {} (mode {mode:o})",
                    from.display(),
                    to.display()
                )
            }
            Self::ReplaceDir { from, to } => {
                write!(f, "replace {} with {}", to.display(), from.display())
            }
            Self::Write { to, mode, .. } => write!(f, "write {} (mode {mode:o})", to.display()),
            Self::Remove(path) => write!(f, "remove {}", path.display()),
            Self::Note(text) => write!(f, "note: {text}"),
        }
    }
}

/// Runs `args` and returns its standard output, or an error with the exit status.
///
/// # Errors
///
/// A message if the program does not start or ends with a nonzero status.
pub fn output(args: &[&str]) -> Result<String, String> {
    let (program, rest) = args.split_first().ok_or("no program")?;
    let out = Command::new(program)
        .args(rest)
        .output()
        .map_err(|error| format!("{program} did not start: {error}"))?;
    if !out.status.success() {
        return Err(format!(
            "`{}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Reports whether `args` runs with exit status 0. Its output goes nowhere.
#[must_use]
pub fn succeeds(args: &[&str]) -> bool {
    args.split_first().is_some_and(|(program, rest)| {
        Command::new(program)
            .args(rest)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// Does each step of `plan` in order. The first error stops the plan.
///
/// # Errors
///
/// A message with the step that failed.
pub fn apply(plan: &[Step]) -> Result<(), String> {
    for step in plan {
        apply_step(step).map_err(|error| format!("{step}: {error}"))?;
    }
    Ok(())
}

fn apply_step(step: &Step) -> Result<(), String> {
    match step {
        Step::Run(args) | Step::TryRun(args) => {
            let (program, rest) = args.split_first().ok_or("no program")?;
            let mut command = Command::new(program);
            command.args(rest);
            // The errors of a TryRun step are expected, for example groupdel of a group
            // that userdel removed. Their messages would only confuse the administrator.
            if matches!(step, Step::TryRun(_)) {
                command.stderr(std::process::Stdio::null());
            }
            let status = command
                .status()
                .map_err(|error| format!("{program} did not start: {error}"))?;
            if !status.success() && matches!(step, Step::Run(_)) {
                return Err(format!("the exit status is {status}"));
            }
        }
        Step::Copy { from, to, mode } => {
            let content = fs::read(from).map_err(|error| error.to_string())?;
            write_atomic(to, &content, *mode).map_err(|error| error.to_string())?;
        }
        Step::ReplaceDir { from, to } => {
            replace_dir(from, to).map_err(|error| error.to_string())?
        }
        Step::Write { to, content, mode } => {
            write_atomic(to, content.as_bytes(), *mode).map_err(|error| error.to_string())?;
        }
        Step::Remove(path) => match fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => {
                fs::remove_dir_all(path).map_err(|error| error.to_string())?
            }
            Ok(_) => fs::remove_file(path).map_err(|error| error.to_string())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        },
        Step::Note(text) => println!("{text}"),
    }
    Ok(())
}

/// Writes `content` to `path` through a new file in the same directory and a rename.
/// A running program at `path` keeps its old file.
fn write_atomic(path: &Path, content: &[u8], mode: u32) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("/"));
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map_or_else(|| "file".into(), |name| name.to_string_lossy().into_owned());
    let temporary = dir.join(format!(".{name}.farglassctl"));
    let _ = fs::remove_file(&temporary);
    fs::write(&temporary, content)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))?;
    fs::rename(&temporary, path)
}

/// Copies the directory `from` to `to`: a new copy first, then a rename, then the old copy goes.
fn replace_dir(from: &Path, to: &Path) -> io::Result<()> {
    let parent = to.parent().unwrap_or_else(|| Path::new("/"));
    fs::create_dir_all(parent)?;
    let name = to
        .file_name()
        .map_or_else(|| "dir".into(), |name| name.to_string_lossy().into_owned());
    let fresh = parent.join(format!(".{name}.new"));
    let old = parent.join(format!(".{name}.old"));
    let _ = fs::remove_dir_all(&fresh);
    let _ = fs::remove_dir_all(&old);
    copy_tree(from, &fresh)?;
    if to.exists() {
        fs::rename(to, &old)?;
    }
    fs::rename(&fresh, to)?;
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    fs::set_permissions(to, fs::Permissions::from_mode(0o755))?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
            fs::set_permissions(&target, fs::Permissions::from_mode(0o644))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_steps_write_copy_replace_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        fs::create_dir_all(source.join("assets")).unwrap();
        fs::write(source.join("index.html"), "new").unwrap();
        fs::write(source.join("assets/app.js"), "js").unwrap();
        let web = dir.path().join("share/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(web.join("old.html"), "old").unwrap();
        let program = dir.path().join("libexec/farglassd");
        apply(&[
            Step::Write {
                to: dir.path().join("etc/conf"),
                content: "a = 1\n".to_owned(),
                mode: 0o644,
            },
            Step::Copy {
                from: source.join("index.html"),
                to: program.clone(),
                mode: 0o755,
            },
            Step::ReplaceDir {
                from: source.clone(),
                to: web.clone(),
            },
            Step::Remove(dir.path().join("etc/conf")),
            Step::Remove(dir.path().join("missing")),
        ])
        .unwrap();
        assert_eq!(
            fs::metadata(&program).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(fs::read_to_string(web.join("assets/app.js")).unwrap(), "js");
        assert!(!web.join("old.html").exists(), "the old directory is gone");
        assert!(!dir.path().join("etc/conf").exists());
    }

    #[test]
    fn a_failed_run_stops_the_plan_and_a_try_run_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("marker");
        let result = apply(&[
            Step::TryRun(vec!["false".to_owned()]),
            Step::Run(vec!["false".to_owned()]),
            Step::Write {
                to: marker.clone(),
                content: String::new(),
                mode: 0o644,
            },
        ]);
        assert!(result.unwrap_err().starts_with("run: false"));
        assert!(!marker.exists(), "a step after the error ran");
    }
}
