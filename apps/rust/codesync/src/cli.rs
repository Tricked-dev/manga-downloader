use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{PlanOptions, SyncOptions, load_config, plan_workflows, sync_workflows};

#[derive(Debug, Parser)]
#[command(name = "codesync", about = "Starlark-configured Git-to-Git code sync")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Load and validate a Starlark codesync config.
    Validate {
        /// Path to the Starlark config file.
        config: PathBuf,
    },
    /// Show commits that would be synchronized.
    Plan {
        /// Path to the Starlark config file.
        config: PathBuf,
        /// Only run one workflow by name.
        #[arg(short, long)]
        workflow: Option<String>,
        /// Directory for temporary clones. A workflow subdirectory is recreated.
        #[arg(long)]
        work_dir: Option<PathBuf>,
    },
    /// Synchronize pending source commits into the destination repository.
    Sync {
        /// Path to the Starlark config file.
        config: PathBuf,
        /// Only run one workflow by name.
        #[arg(short, long)]
        workflow: Option<String>,
        /// Directory for temporary clones. A workflow subdirectory is recreated.
        #[arg(long)]
        work_dir: Option<PathBuf>,
        /// Push destination commits after a successful sync.
        #[arg(long)]
        push: bool,
        /// Print the plan without writing commits.
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run() -> Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Validate { config } => {
            let config = load_config(&config)?;
            println!("loaded {} workflow(s)", config.workflows.len());
            for workflow in config.workflows {
                println!("  {}", workflow.name);
            }
        }
        Command::Plan {
            config,
            workflow,
            work_dir,
        } => {
            let config = load_config(&config)?;
            let plans = plan_workflows(&config, &PlanOptions { workflow, work_dir })?;
            print_plans(&plans);
        }
        Command::Sync {
            config,
            workflow,
            work_dir,
            push,
            dry_run,
        } => {
            let config = load_config(&config)?;
            if dry_run {
                let plans = plan_workflows(&config, &PlanOptions { workflow, work_dir })?;
                print_plans(&plans);
                return Ok(());
            }

            let reports = sync_workflows(
                &config,
                &SyncOptions {
                    workflow,
                    work_dir,
                    push,
                },
            )?;
            for report in reports {
                println!(
                    "{}: {} committed, {} empty, {} pending source commit(s)",
                    report.workflow, report.committed, report.empty_commits, report.pending
                );
            }
        }
    }

    Ok(())
}

fn print_plans(plans: &[crate::WorkflowPlan]) {
    for plan in plans {
        println!("workflow {}", plan.workflow);
        println!("  mode: {:?}", plan.mode);
        println!("  source head: {}", plan.source_head);
        match &plan.last_synced {
            Some(rev) => println!("  last synced: {rev}"),
            None => println!("  last synced: <none>"),
        }
        println!("  pending: {}", plan.commits.len());
        if let Some(pr) = &plan.pull_request {
            match &pr.branch {
                Some(branch) => println!("  pull request branch: {branch}"),
                None => println!("  pull request branch: <auto>"),
            }
            if let Some(title) = &pr.title {
                println!("  pull request title: {title}");
            }
            if pr.draft {
                println!("  pull request draft: true");
            }
            if !pr.assignees.is_empty() {
                println!("  pull request assignees: {}", pr.assignees.join(", "));
            }
            if !pr.labels.is_empty() {
                println!("  pull request labels: {}", pr.labels.join(", "));
            }
        }
        for commit in &plan.commits {
            println!("    {} {}", commit.short_sha(), commit.subject);
        }
    }
}
