// Copyright 2024-2025 Irreducible Inc.

use chrono::{Local, TimeZone, Utc};
use git2::{Repository, StatusOptions};

/// Sample `Local::now()` once and return a pair:
/// 1) `YYYYMMDDTHHmmss` for filenames (ISO 8601 basic format)
/// 2) full RFC3339/ISO timestamp for metadata
pub fn get_formatted_time() -> (String, String) {
    let now = Local::now();
    (now.format("%Y%m%dT%H%M%S").to_string(), now.to_rfc3339())
}

pub fn sanitize_filename(branch: &str) -> String {
    branch
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | ' ' | '.' => '-',
            _ => c,
        })
        .collect()
}

/// Information about the current Git repository HEAD.
#[derive(Debug)]
pub struct GitInfo {
    /// Current branch name.
    pub branch: String,
    /// Short (7-char) commit hash.
    pub commit_short: String,
    /// Full commit message (first line), if available.
    #[cfg_attr(not(feature = "perfetto"), allow(dead_code))]
    pub commit_message: Option<String>,
    /// Commit author name, if available.
    #[cfg_attr(not(feature = "perfetto"), allow(dead_code))]
    pub commit_author: Option<String>,
    /// Commit timestamp (ISO-8601), if available.
    #[cfg_attr(not(feature = "perfetto"), allow(dead_code))]
    pub commit_time: Option<String>,
    /// Whether the working tree is clean (no uncommitted changes).
    pub is_clean: bool,
}

/// Discovers the repository and returns a `GitInfo` snapshot, or `None` on any error.
pub fn get_git_info() -> Option<GitInfo> {
    let repo = Repository::discover(".").ok()?;
    let head = repo.head().ok()?;
    let branch = head.shorthand().ok()?.to_string();
    let commit = head.peel_to_commit().ok()?;
    let commit_short = commit.id().to_string()[..7].to_string();
    let commit_message = commit.message().ok().map(|m| m.trim().to_string());
    let commit_author = commit.author().name().ok().map(|s| s.to_string());

    // Format the commit timestamp as ISO-8601
    let commit_time = {
        let t = commit.time();
        Utc.timestamp_opt(t.seconds(), 0)
            .single()
            .map(|dt_utc| dt_utc.with_timezone(&Local).to_rfc3339())
    };

    // Is clean
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let is_clean = repo
        .statuses(Some(&mut opts))
        .map(|statuses| statuses.is_empty())
        .unwrap_or(false);

    Some(GitInfo {
        branch,
        commit_short,
        commit_message,
        commit_author,
        commit_time,
        is_clean,
    })
}
