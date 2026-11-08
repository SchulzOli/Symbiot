//! The history of a local Git repository, read with the `git` command.
//!
//! One item per commit on any branch. The id names the repository by its `origin` remote
//! (`github.com/owner/repo`), so clones in different places give the same ids.

use std::path::Path;
use std::process::Command;

use symbiot_core::evidence::{Actor, Evidence, role};

use crate::{Error, Result};

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

pub fn read(repo: &Path) -> Result<Vec<Evidence>> {
    let format = ["%H", "%at", "%an", "%ae", "%cn", "%ce", "%s", "%b"].join("%x1f") + "%x1e";
    let log = git(
        repo,
        &["log", "--all", "--no-color", &format!("--format={format}")],
    )?;
    let name = repo_name(repo);
    let mut out = Vec::new();
    for record in log.split(RECORD) {
        let f: Vec<&str> = record.trim_start_matches('\n').split(FIELD).collect();
        let [sha, at, an, ae, cn, ce, subject, body] = f[..] else {
            continue;
        };
        let Some(at_ms) = at.parse::<u64>().ok().map(|s| s * 1000).filter(|&t| t > 0) else {
            continue;
        };
        let mut actors = vec![person(ae, an, role::AUTHOR)];
        // Merges through the web show GitHub itself as committer; that is no person.
        if ce != ae && ce != "noreply@github.com" {
            actors.push(person(ce, cn, role::PARTICIPANT));
        }
        actors.retain(|a| a.id.len() > "mailto:".len());
        let body = body.trim();
        let text = if body.is_empty() {
            subject.to_string()
        } else {
            format!("{subject}\n\n{body}")
        };
        out.push(Evidence {
            id: format!("git:{name}@{sha}"),
            kind: "symbiot:Commit".into(),
            at_ms,
            actors,
            text,
            fields: vec![
                ("repo".into(), name.clone()),
                ("subject".into(), subject.into()),
            ],
            files: vec![],
        });
    }
    Ok(out)
}

fn person(email: &str, name: &str, role: &str) -> Actor {
    Actor {
        id: format!("mailto:{}", email.trim().to_lowercase()),
        role: role.into(),
        name: name.trim().into(),
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| Error::Git(format!("nicht ausführbar ({e})")))?;
    if !out.status.success() {
        return Err(Error::Git(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `github.com/owner/repo` from the `origin` remote, else the folder name.
fn repo_name(repo: &Path) -> String {
    let url = git(repo, &["remote", "get-url", "origin"]).unwrap_or_default();
    let url = url.trim();
    if !url.is_empty() {
        let scheme = url.contains("://");
        let s = url.split_once("://").map_or(url, |(_, rest)| rest);
        let s = s.split_once('@').map_or(s, |(_, rest)| rest);
        // scp-like `host:owner/repo` becomes `host/owner/repo`.
        let s = if scheme {
            s.to_string()
        } else {
            s.replacen(':', "/", 1)
        };
        return s.trim_end_matches('/').trim_end_matches(".git").to_string();
    }
    std::fs::canonicalize(repo)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "repo".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_DATE", "2024-03-12T14:05:00Z")
            .env("GIT_COMMITTER_DATE", "2024-03-12T14:05:00Z")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn commits_of_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        run(d, &["init", "-q", "-b", "main"]);
        run(d, &["config", "user.name", "Anna Beispiel"]);
        run(d, &["config", "user.email", "Anna@Example.org"]);
        run(
            d,
            &["commit", "-q", "--allow-empty", "-m", "Erster Schritt"],
        );
        run(
            d,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "Zweiter",
                "-m",
                "mit Text",
            ],
        );
        run(
            d,
            &[
                "remote",
                "add",
                "origin",
                "git@github.com:beispiel/projekt.git",
            ],
        );

        let items = read(d).unwrap();
        assert_eq!(items.len(), 2);
        let newest = &items[0];
        assert!(newest.id.starts_with("git:github.com/beispiel/projekt@"));
        assert_eq!(newest.text, "Zweiter\n\nmit Text");
        assert_eq!(newest.at_ms, 1_710_252_300_000);
        assert_eq!(newest.actors.len(), 1);
        assert_eq!(newest.actors[0].id, "mailto:anna@example.org");
        assert_eq!(newest.actors[0].name, "Anna Beispiel");
        assert!(items.iter().all(|e| e.check().is_ok()));
    }

    #[test]
    fn repo_names() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        run(d, &["init", "-q"]);
        for (url, want) in [
            ("https://github.com/a/b.git", "github.com/a/b"),
            ("git@github.com:a/b.git", "github.com/a/b"),
            ("ssh://git@host.example/a/b", "host.example/a/b"),
        ] {
            let _ = Command::new("git")
                .arg("-C")
                .arg(d)
                .args(["remote", "remove", "origin"])
                .output();
            run(d, &["remote", "add", "origin", url]);
            assert_eq!(repo_name(d), want, "{url}");
        }
    }

    #[test]
    fn not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(read(dir.path()), Err(Error::Git(_))));
    }
}
