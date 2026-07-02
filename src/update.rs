use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

pub const BUILD_COMMIT: &str = env!("IDRAW_COMMIT");
pub const REPO_DIR: &str = env!("CARGO_MANIFEST_DIR");
pub const ORIGIN_URL: &str = env!("IDRAW_ORIGIN");

#[derive(Clone, Debug)]
pub enum UpdateStatus {
    Checking,
    UpToDate,
    Available { commit: String },
    CheckFailed(#[allow(dead_code)] String), // reason kept for Debug/diagnostics
}

/// git command that can never prompt/hang. Only sets current_dir(REPO_DIR)
/// when `in_repo` is true, so a missing/unusable REPO_DIR never causes a
/// spawn error for URL-based calls.
fn git(args: &[&str], in_repo: bool) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if in_repo {
        cmd.current_dir(REPO_DIR);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env(
        "GIT_SSH_COMMAND",
        "ssh -oBatchMode=yes -oConnectTimeout=5",
    );
    cmd
}

/// The build-time checkout is usable iff it exists, is a git work tree with an
/// `origin` remote, and is not a cargo cache checkout (path contains
/// ".cargo/git/checkouts" or ".cargo\\git\\checkouts").
fn local_repo_usable() -> bool {
    if REPO_DIR.contains(".cargo/git/checkouts") || REPO_DIR.contains(".cargo\\git\\checkouts") {
        return false;
    }
    // Custom CARGO_HOME cache checkouts don't contain ".cargo" — check at runtime.
    if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
        let cache = PathBuf::from(cargo_home).join("git").join("checkouts");
        if std::path::Path::new(REPO_DIR).starts_with(&cache) {
            return false;
        }
    }
    if !std::path::Path::new(REPO_DIR).is_dir() {
        return false;
    }
    let is_work_tree = git(&["rev-parse", "--is-inside-work-tree"], true)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !is_work_tree {
        return false;
    }
    git(&["remote", "get-url", "origin"], true)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Cargo (unlike the git CLI) only accepts real URLs: normalize scp-like
/// `user@host:path` to `ssh://user@host/path` and bare filesystem paths to
/// `file://...`; anything already carrying a scheme passes through.
fn cargo_git_url(origin: &str) -> String {
    if origin.contains("://") {
        origin.to_string()
    } else if let Some((host, path)) = origin.split_once(':').filter(|_| !origin.starts_with('/')) {
        format!("ssh://{}/{}", host, path.trim_start_matches('/'))
    } else if origin.starts_with('/') {
        format!("file://{origin}")
    } else {
        origin.to_string()
    }
}

fn check() -> UpdateStatus {
    if BUILD_COMMIT == "unknown" {
        return UpdateStatus::CheckFailed("built outside git".to_string());
    }

    let local = local_repo_usable();

    let output = if local {
        git(&["ls-remote", "origin", "HEAD"], true).output()
    } else if ORIGIN_URL == "unknown" {
        return UpdateStatus::CheckFailed("no repo checkout and no origin url".to_string());
    } else {
        git(&["ls-remote", ORIGIN_URL, "HEAD"], false).output()
    };

    let output = match output {
        Ok(o) => o,
        Err(e) => return UpdateStatus::CheckFailed(e.to_string().trim().to_string()),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.lines().next().unwrap_or("git ls-remote failed").trim();
        return UpdateStatus::CheckFailed(msg.to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let remote = match stdout.lines().next().and_then(|l| l.split_whitespace().next()) {
        Some(hash) if !hash.is_empty() => hash.to_string(),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let msg = stderr.lines().next().unwrap_or("unparsable ls-remote output").trim();
            return UpdateStatus::CheckFailed(msg.to_string());
        }
    };

    if remote == BUILD_COMMIT {
        return UpdateStatus::UpToDate;
    }

    if local {
        let is_ancestor = git(&["merge-base", "--is-ancestor", &remote, "HEAD"], true)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if is_ancestor {
            return UpdateStatus::UpToDate;
        }
    }

    UpdateStatus::Available { commit: remote }
}

/// Spawn a background check thread; it sends exactly one final UpdateStatus.
pub fn spawn_check() -> Receiver<UpdateStatus> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(check());
    });
    rx
}

/// Blocking update. Call ONLY after leaving the TUI (raw mode off).
///
/// If the build-time checkout is usable, runs `git pull --ff-only` then
/// `cargo install --path REPO_DIR`, both with Stdio::inherit() so the user
/// sees progress. Otherwise, if an origin URL was embedded at build time,
/// falls back to `cargo install --git <ORIGIN_URL>`. If neither is
/// available, bails with a clear message.
pub fn perform_update() -> anyhow::Result<PathBuf> {
    if local_repo_usable() {
        let status = git(&["pull", "--ff-only"], true)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            anyhow::bail!("git pull --ff-only failed");
        }

        let status = Command::new("cargo")
            .args(["install", "--path", REPO_DIR])
            .current_dir(REPO_DIR)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            anyhow::bail!("cargo install --path {REPO_DIR} failed");
        }
    } else if ORIGIN_URL != "unknown" {
        let url = cargo_git_url(ORIGIN_URL);
        let status = Command::new("cargo")
            .args(["install", "--git", &url])
            .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes -oConnectTimeout=5")
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        if !status.success() {
            anyhow::bail!("cargo install --git {url} failed");
        }
    } else {
        anyhow::bail!(
            "cannot update: build-time repo checkout is gone and no origin url is embedded"
        );
    }

    let cargo_bin_dir = match std::env::var_os("CARGO_HOME") {
        Some(home) => PathBuf::from(home).join("bin"),
        None => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("HOME environment variable not set"))?;
            PathBuf::from(home).join(".cargo").join("bin")
        }
    };
    let bin = cargo_bin_dir.join("idraw");

    if !bin.exists() {
        anyhow::bail!("expected installed binary at {} but it does not exist", bin.display());
    }

    Ok(bin)
}
