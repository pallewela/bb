mod api;
mod auth;
mod browse;
mod client;
mod config;
mod context;
mod exit;
mod output;
mod pipeline;
mod pr;
mod repo;
mod skill;
mod workspace;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bb", version, about = "Work with Bitbucket Cloud from the command line")]
struct Cli {
    /// Select another repository: WORKSPACE/REPO or a bitbucket.org URL
    #[arg(short = 'R', long = "repo", id = "repo_flag", global = true, value_name = "WORKSPACE/REPO")]
    repo: Option<String>,
    /// Print raw API JSON instead of human-readable output
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Manage credentials
    Auth {
        #[command(subcommand)]
        cmd: auth::Cmd,
    },
    /// Work with repositories
    Repo {
        #[command(subcommand)]
        cmd: repo::Cmd,
    },
    /// Work with workspaces
    #[command(alias = "ws", visible_alias = "org")]
    Workspace {
        #[command(subcommand)]
        cmd: workspace::Cmd,
    },
    /// Work with pull requests
    Pr {
        #[command(subcommand)]
        cmd: pr::Cmd,
    },
    /// Work with Bitbucket Pipelines
    #[command(alias = "run")]
    Pipeline {
        #[command(subcommand)]
        cmd: pipeline::Cmd,
    },
    /// Run custom pipelines (the `gh workflow` analog)
    Workflow {
        #[command(subcommand)]
        cmd: pipeline::WorkflowCmd,
    },
    /// Make an authenticated API request
    Api(api::Args),
    /// Open the repository in the browser
    Browse(browse::Args),
    /// Install the agent skill that teaches AI coding agents to use bb
    Skill {
        #[command(subcommand)]
        cmd: skill::Cmd,
    },
}

fn main() {
    let cli = Cli::parse();
    context::set_repo_flag(cli.repo);
    output::set_json(cli.json);
    let res = match cli.cmd {
        Cmd::Auth { cmd } => auth::run(cmd),
        Cmd::Repo { cmd } => repo::run(cmd),
        Cmd::Workspace { cmd } => workspace::run(cmd),
        Cmd::Pr { cmd } => pr::run(cmd),
        Cmd::Pipeline { cmd } => pipeline::run(cmd),
        Cmd::Workflow { cmd } => pipeline::workflow(cmd),
        Cmd::Api(args) => api::run(args),
        Cmd::Browse(args) => browse::run(args),
        Cmd::Skill { cmd } => skill::run(cmd),
    };
    if let Err(e) = res {
        eprintln!("bb: {e:#}");
        std::process::exit(exit::code(&e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_flag_works_on_subcommands_with_a_repo_positional() {
        for args in [["bb", "repo", "view", "-R", "ws/r"], ["bb", "repo", "delete", "-R", "ws/r"]] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert_eq!(cli.repo.as_deref(), Some("ws/r"));
        }
    }

    #[test]
    fn pr_merge_strategy_flags_are_exclusive() {
        assert!(Cli::try_parse_from(["bb", "pr", "merge", "1", "--rebase-merge"]).is_ok());
        assert!(Cli::try_parse_from(["bb", "pr", "merge", "1", "--squash-fast-forward"]).is_ok());
        for args in [
            ["bb", "pr", "merge", "1", "--rebase-merge", "-s"],
            ["bb", "pr", "merge", "1", "--squash-fast-forward", "--rebase-merge"],
        ] {
            assert_eq!(Cli::try_parse_from(args).err().unwrap().kind(), clap::error::ErrorKind::ArgumentConflict);
        }
    }
}
