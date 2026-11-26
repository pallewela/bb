//! `bb skill install`: write the agent skill (skills/bitbucket-cli/SKILL.md, built into the binary)
//! where Claude Code finds it, so installs from a release or package get it without a clone.
use anyhow::{Context, Result, bail};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const NAME: &str = "bitbucket-cli";
const SKILL: &str = include_str!("../skills/bitbucket-cli/SKILL.md");

#[derive(clap::Subcommand)]
pub enum Cmd {
    /// Install or update the agent skill for Claude Code
    Install {
        /// Skills directory to install into (default: $CLAUDE_CONFIG_DIR/skills, else ~/.claude/skills)
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
}

pub fn run(cmd: Cmd) -> Result<()> {
    let Cmd::Install { dir } = cmd;
    let dir = dir.unwrap_or_else(|| skills_dir(|k| std::env::var_os(k), cfg!(windows)));
    let (path, verb) = install(&dir)?;
    println!("{verb} {}", path.display());
    Ok(())
}

/// Claude Code's user skills directory: `$CLAUDE_CONFIG_DIR/skills`, else `~/.claude/skills`.
fn skills_dir(var: impl Fn(&str) -> Option<OsString>, windows: bool) -> PathBuf {
    let get = |k: &str| var(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = get("CLAUDE_CONFIG_DIR") {
        return dir.join("skills");
    }
    let home = get("HOME").or_else(|| if windows { get("USERPROFILE") } else { None }).unwrap_or_default();
    home.join(".claude").join("skills")
}

/// Writes `dir/bitbucket-cli/SKILL.md`; returns its path and what happened.
fn install(dir: &Path) -> Result<(PathBuf, &'static str)> {
    let skill_dir = dir.join(NAME);
    // A symlink here usually points into a clone of bb (the old install method); writing through
    // it would overwrite that clone's SKILL.md with this binary's copy.
    if let Ok(target) = fs::read_link(&skill_dir) {
        bail!("{} is a symlink to {}; remove it to install this copy", skill_dir.display(), target.display());
    }
    let path = skill_dir.join("SKILL.md");
    let verb = match fs::read_to_string(&path) {
        Ok(old) if old == SKILL => return Ok((path, "Already up to date:")),
        Ok(_) => "Updated",
        Err(_) => "Installed",
    };
    fs::create_dir_all(&skill_dir).with_context(|| format!("creating {}", skill_dir.display()))?;
    fs::write(&path, SKILL).with_context(|| format!("writing {}", path.display()))?;
    Ok((path, verb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir() {
        let env = |pairs: &'static [(&str, &str)]| move |k: &str| pairs.iter().find(|p| p.0 == k).map(|p| p.1.into());
        assert_eq!(skills_dir(env(&[("HOME", "/h"), ("CLAUDE_CONFIG_DIR", "/c")]), false), Path::new("/c/skills"));
        assert_eq!(
            skills_dir(env(&[("HOME", "/h"), ("CLAUDE_CONFIG_DIR", "")]), false),
            Path::new("/h/.claude/skills")
        );
        assert_eq!(skills_dir(env(&[("USERPROFILE", "/u")]), true), Path::new("/u/.claude/skills"));
    }

    #[test]
    fn installs_updates_and_refuses_symlinks() {
        let dir = std::env::temp_dir().join(format!("bb-skill-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (path, verb) = install(&dir).unwrap();
        assert_eq!((verb, fs::read_to_string(&path).unwrap().as_str()), ("Installed", SKILL));
        assert_eq!(install(&dir).unwrap().1, "Already up to date:");
        fs::write(&path, "old").unwrap();
        assert_eq!(install(&dir).unwrap().1, "Updated");
        assert_eq!(fs::read_to_string(&path).unwrap(), SKILL);

        #[cfg(unix)]
        {
            let link_dir = dir.join("links");
            fs::create_dir_all(&link_dir).unwrap();
            std::os::unix::fs::symlink(dir.join(NAME), link_dir.join(NAME)).unwrap();
            assert!(install(&link_dir).unwrap_err().to_string().contains("is a symlink"));
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
