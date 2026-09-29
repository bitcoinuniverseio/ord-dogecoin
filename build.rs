//! Embed the source commit the binary was built from as
//! `UNIVERSE_PROVIDER_COMMIT`, reported by the Dogemap feed as
//! `providerCommit`. A release build outside a Git checkout sets the variable
//! explicitly; without either the value is `unknown` and the feed reports
//! itself not ready rather than claiming a provenance it cannot prove.

use std::{path::Path, process::Command};

fn git(args: &[&str]) -> Option<String> {
  let output = Command::new("git").args(args).output().ok()?;
  if !output.status.success() {
    return None;
  }
  Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}

fn full_commit(value: &str) -> bool {
  value.len() == 40
    && value
      .bytes()
      .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn main() {
  println!("cargo:rerun-if-env-changed=UNIVERSE_PROVIDER_COMMIT");

  let commit = match std::env::var("UNIVERSE_PROVIDER_COMMIT") {
    Ok(value) if !value.is_empty() => Some(value),
    _ => {
      // Rebuild when HEAD moves or the branch it names advances.
      for path in [
        git(&["rev-parse", "--git-path", "HEAD"]),
        git(&["rev-parse", "--symbolic-full-name", "HEAD"])
          .and_then(|reference| git(&["rev-parse", "--git-path", &reference])),
        git(&["rev-parse", "--git-path", "packed-refs"]),
      ]
      .into_iter()
      .flatten()
      {
        if Path::new(&path).exists() {
          println!("cargo:rerun-if-changed={path}");
        }
      }
      git(&["rev-parse", "HEAD"])
    }
  };

  let commit = commit
    .map(|commit| commit.to_ascii_lowercase())
    .filter(|commit| full_commit(commit))
    .unwrap_or_else(|| "unknown".to_owned());

  println!("cargo:rustc-env=UNIVERSE_PROVIDER_COMMIT={commit}");
}
