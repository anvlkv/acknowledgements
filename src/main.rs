use std::{collections::HashSet, path::PathBuf, time::Duration};

use cargo_toml::{Dependency, Manifest};
use clap::Parser;
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{sleep_until, Instant},
};
use unfmt_macros::unformat;

const USER_AGENT: &str = "acknowledgments.rs (acknowledgements_rs@proton.me)";
const RATE_LIMIT: u64 = 1000;
const GITHUB_BASE: &str = "https://github.com";
const GITHUB_AT_GIT: &str = "git@github.com";
const TEMPLATE: &str = include_str!("./template.md");

/// Acknowledgements is a simple CLI tool
/// to analyze dependencies of a Cargo (rust) project
/// and produce an ACKNOWLEDMENTS.md file
/// listing (major) contributors of your dependencies
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to Cargo project for analysis
    #[arg(short, long)]
    path: PathBuf,

    /// Output file path, defaults to project path if not provided
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Format of the output file
    #[arg(short, long, default_value_t = Format::NameAndCount)]
    format: Format,

    /// Depth of scan, whether to include minor and optional depes contributors
    #[arg(short, long, default_value_t = Depth::Major)]
    depth: Depth,

    /// List other sources, not specified in Cargo.toml
    #[arg(short, long)]
    sources: Vec<String>,
}

#[derive(Debug, Clone, Copy, strum_macros::Display, strum_macros::EnumString)]
enum Format {
    NameAndCount,
    DepAndNames,
    NameAndDeps,
}

#[derive(Debug, Clone, Copy, strum_macros::Display, strum_macros::EnumString)]
enum Depth {
    Major,
    Direct,
    Indepth,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut github_sources: HashSet<String> = args
        .sources
        .iter()
        .filter_map(|s| {
            if s.starts_with(GITHUB_AT_GIT) {
                Some(s.replace(GITHUB_AT_GIT, GITHUB_BASE))
            } else if s.starts_with(GITHUB_BASE) {
                Some(s.clone())
            } else {
                None
            }
        })
        .collect();

    let mut other_sources: HashSet<String> = args
        .sources
        .iter()
        .filter(|s| !s.starts_with(GITHUB_AT_GIT) && !s.starts_with(GITHUB_BASE))
        .cloned()
        .collect();

    let deps = manifest_deps(&args.path, &args.depth)?;

    let mut fetch_deps_data = HashSet::new();

    for (name, dep) in deps {
        match dep {
            Dependency::Detailed(detail) => {
                if let Some(git) = detail.git {
                    if git.starts_with("https://github.com") || git.starts_with("git@github.com") {
                        _ = github_sources
                            .insert(git.replace("git@github.com", "https://github.com"));
                    } else {
                        eprintln!("source not supported: {git}")
                    }
                } else if detail.path.is_none() {
                    fetch_deps_data.insert(name);
                }
            }
            _ => {
                fetch_deps_data.insert(name);
            }
        }
    }

    let (repo_sx, mut repo_rx) = unbounded_channel();

    let out = tokio::spawn(async move {
        let crates_io_client = crates_io_api::AsyncClient::new(
            USER_AGENT,
            std::time::Duration::from_millis(RATE_LIMIT),
        )?;

        for crate_name in fetch_deps_data {
            let start = Instant::now();
            println!("fetching data for: {crate_name}");

            let data = crates_io_client.get_crate(crate_name.as_str()).await?;

            if let Some(r) = data.crate_data.repository {
                repo_sx.send((crate_name, r))?;
            }

            if Instant::now().duration_since(start).as_millis() < RATE_LIMIT as u128 {
                sleep_until(
                    start
                        .checked_add(Duration::from_millis(RATE_LIMIT))
                        .unwrap(),
                )
                .await;
            }
        }

        anyhow::Ok(())
    });

    while let Some((name, git)) = repo_rx.recv().await {
        if git.starts_with(GITHUB_BASE) || git.starts_with(GITHUB_AT_GIT) {
            _ = github_sources.insert(git.replace(GITHUB_AT_GIT, GITHUB_BASE));
        } else {
            _ = other_sources.insert(git);
        }
    }

    _ = out.await?;

    let (contrib_sx, mut contrib_rx) = unbounded_channel();

    let out = tokio::spawn(async move {
        let github_client = octocrab::instance();
        for src in github_sources {
            let parsed = unformat!("https://github.com/{}/{}", &src);

            if let Some((owner, repo)) = parsed {
                let repo_data = github_client.repos(owner, repo);
                let first = repo_data.list_contributors().send().await?;

                for c in first.items.iter() {
                    contrib_sx.send(c.clone())?;
                }

                if let Some(pages) = first.number_of_pages() {
                    for page in 0..pages {
                        let next = repo_data.list_contributors().page(page).send().await?;
                        for c in next.items {
                            contrib_sx.send(c)?;
                        }
                    }
                }
            }
        }

        anyhow::Ok(())
    });

    let mut contributors = Vec::new();

    while let Some(c) = contrib_rx.recv().await {
        contributors.push(c);
    }

    _ = out.await?;

    println!("contributors: {contributors:?}");

    Ok(())
}

fn manifest_deps(path: &PathBuf, depth: &Depth) -> anyhow::Result<Vec<(String, Dependency)>> {
    let manifest = Manifest::from_path(path.as_path()).or_else(|_| {
        let mut path = path.clone();
        path.push("Cargo.toml");
        Manifest::from_path(path.as_path())
    })?;

    let mut deps: Vec<_> = match depth {
        Depth::Major => manifest
            .dependencies
            .iter()
            .filter(|d| !d.1.optional())
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect(),
        Depth::Direct => manifest
            .dependencies
            .iter()
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect(),
        Depth::Indepth => manifest
            .dependencies
            .iter()
            .chain(manifest.dev_dependencies.iter())
            .chain(manifest.build_dependencies.iter())
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect(),
    };

    if let Some(workspace) = manifest.workspace {
        match depth {
            Depth::Indepth => deps.extend(
                workspace
                    .dependencies
                    .iter()
                    .map(|(k, d)| (k.clone(), d.clone())),
            ),
            _ => deps.extend(
                workspace
                    .dependencies
                    .iter()
                    .filter(|d| !d.1.optional())
                    .map(|(k, d)| (k.clone(), d.clone())),
            ),
        }

        for member in workspace.members.iter() {
            let mut member_path = path.clone();
            member_path.push(member);
            deps.extend(manifest_deps(&member_path, depth)?);
        }
    }

    Ok(deps)
}
