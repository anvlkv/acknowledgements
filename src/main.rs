use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::PathBuf,
};

use cargo_toml::{Dependency, Manifest};
use clap::{Parser, Subcommand};
use handlebars::Handlebars;
use octocrab::models::RateLimit;
use serde::{Deserialize, Serialize};
use tokio::{
    fs,
    sync::mpsc::unbounded_channel,
    time::{sleep, sleep_until, Duration, Instant},
};
use unfmt_macros::unformat;

const USER_AGENT: &str = "acknowledgments.rs (acknowledgements_rs@proton.me)";
const CRATES_IO_RATE_LIMIT: u64 = 1000;
const GITHUB_BASE: &str = "https://github.com";
const GITHUB_AT_GIT: &str = "git@github.com";
const TEMPLATE: &str = include_str!("./template.md");
const CACHE_NAME: &str = "acknowledgements_cache";
const FILE_NAME: &str = "ACKNOWLEDGEMENTS.md";

/// acknowledge is a simple CLI tool
/// to analyze dependencies of a Cargo (rust) project
/// and produce an ACKNOWLEDMENTS.md file
/// listing (major) contributors of your dependencies
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to Cargo project for analysis
    #[arg(short, long)]
    path: PathBuf,

    /// Running Acknowledgements on any project of reasonable size you're likely to face rate limits. Please provide a personal access token.
    #[arg(short, long)]
    gh_token: Option<String>,

    /// Output file path, defaults to project path if not provided
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Whether to include @ (at) symbol in front of a github user's name
    #[arg(short, long, default_value_t = false)]
    mention: bool,

    /// Format of the output file
    #[arg(short, long, default_value_t = Format::NameAndCount)]
    format: Format,

    /// Breadth of scan, whether to include optional, build and dev deps contributors
    #[arg(short, long, default_value_t = Breadth::NonOpt)]
    breadth: Breadth,

    /// Min number of contributions to be included in the list, doesn't apply to sole contributors
    #[arg(short, long, default_value_t = 2)]
    contributions_threshold: usize,

    /// List other sources, not specified in Cargo.toml
    #[arg(short, long)]
    sources: Vec<String>,

    /// Use your own template.
    /// See https://github.com/anvlkv/acknowledgements/blob/main/src/template.md?plain=1
    /// for reference
    #[arg(short, long)]
    template: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Clears cache
    ClearCache,
}

#[derive(Debug, Clone, Copy, strum_macros::Display, strum_macros::EnumString)]
enum Format {
    /// Name of the contributor and count of contributions
    NameAndCount,
    /// Name of the dependency, names of contributors
    DepAndNames,
    /// Name of the contributor, names of dependencies where they contributed
    NameAndDeps,
}

#[derive(Debug, Clone, Copy, strum_macros::Display, strum_macros::EnumString)]
enum Breadth {
    /// Non-optional dependencies
    NonOpt,
    /// All dependencies
    All,
    /// All dependencies including [build-dependencies] and [dev-dependencies]
    BuildAndDev,
}

#[derive(Serialize, Deserialize)]
struct GitLabContributor {
    name: String,
    commits: u32,
}

#[derive(Serialize, Deserialize)]
struct GitLabProject {
    name: String,
}

#[derive(Serialize, Deserialize)]
struct TemplateData {
    thank: Vec<ThankData>,
    others: usize,
    mention: bool,
}

#[derive(Serialize, Deserialize)]
enum ThankData {
    NameAndCount {
        name: String,
        profile_url: String,
        count: usize,
    },
    DepAndNames {
        crate_name: String,
        contributors: BTreeSet<(String, String)>,
    },
    NameAndDeps {
        name: String,
        profile_url: String,
        crates: BTreeSet<String>,
    },
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(_) => println!("Done!"),
        Err(e) => eprintln!("Error: {e:?}"),
    }
}

async fn run() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(command) = args.command {
        match command {
            Commands::ClearCache => return clear_cache().await,
        }
    }

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

    let deps = manifest_deps(&args.path, &args.breadth)?;

    println!("Analyzing {} dependencies...", deps.len());

    let mut fetch_deps_data = HashSet::new();

    for (name, dep) in deps {
        match dep {
            Dependency::Detailed(detail) => {
                if let Some(git) = detail.git {
                    if git.starts_with("https://github.com") || git.starts_with("git@github.com") {
                        _ = github_sources
                            .insert(git.replace("git@github.com", "https://github.com"));
                    } else {
                        _ = other_sources.insert(git)
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
            std::time::Duration::from_millis(CRATES_IO_RATE_LIMIT),
        )?;

        for crate_name in fetch_deps_data {
            let c_key = format!("crates-io, {crate_name}");

            if let Some(d) = read_cached(c_key.as_str()).await {
                repo_sx.send(d)?;
                println!("cached crates.io data for: {crate_name}");
            } else {
                let start = Instant::now();
                println!("fetching crates.io data for: {crate_name}");

                let data = crates_io_client.get_crate(crate_name.as_str()).await?;

                if let Some(r) = data.crate_data.repository {
                    write_cached(c_key.as_str(), r.clone()).await;
                    repo_sx.send(r)?;
                }

                if Instant::now().duration_since(start).as_millis() < CRATES_IO_RATE_LIMIT as u128 {
                    sleep_until(
                        start
                            .checked_add(Duration::from_millis(CRATES_IO_RATE_LIMIT))
                            .unwrap(),
                    )
                    .await;
                }
            }
        }

        anyhow::Ok(())
    });

    while let Some(git) = repo_rx.recv().await {
        if git.starts_with(GITHUB_BASE) || git.starts_with(GITHUB_AT_GIT) {
            _ = github_sources.insert(git.replace(GITHUB_AT_GIT, GITHUB_BASE).replace(".git", ""));
        } else {
            _ = other_sources.insert(git.replace(".git", ""));
        }
    }

    _ = out.await??;

    let (contrib_sx, mut contrib_rx) = unbounded_channel();

    let gh_token = args
        .gh_token
        .or(read_cached::<Option<String>>("github_access_token")
            .await
            .flatten());

    if gh_token.is_none() {
        println!("Starting without github access token, may take longer...");
    }

    let out_gh = tokio::spawn({
        let contrib_sx = contrib_sx.clone();
        async move {
            println!("{} github.com sources...", github_sources.len());

            let github_client = if let Some(tok) = gh_token {
                std::sync::Arc::new(
                    octocrab::OctocrabBuilder::new()
                        .personal_token(tok)
                        .build()?,
                )
            } else {
                octocrab::instance()
            };

            for src in github_sources {
                if let Some((data, contributors)) = read_cached::<(
                    octocrab::models::Repository,
                    Vec<octocrab::models::Contributor>,
                )>(&src)
                .await
                {
                    println!("cached github.com data for: {src}");

                    for c in contributors {
                        contrib_sx.send((
                            data.name.clone(),
                            c.author.login.clone(),
                            c.author.html_url.to_string(),
                            c.contributions,
                        ))?;
                    }
                } else {
                    let parsed = unformat!("https://github.com/{}/{}", &src);

                    if let Some((owner, repo)) = parsed {
                        // split-off any monorepo paths
                        let repo = repo.split("/").next().unwrap_or(repo);

                        println!("fetching github.com data for: {owner} {repo}");

                        let mut contributors = vec![];
                        let repo_handler = github_client.repos(owner, repo);
                        let mut limit = gh_rate_limited(None, &github_client).await?;
                        let data = repo_handler.get().await?;
                        limit = gh_rate_limited(Some(limit), &github_client).await?;
                        let first = repo_handler.list_contributors().send().await?;

                        for c in first.items.iter() {
                            contrib_sx.send((
                                data.name.clone(),
                                c.author.login.clone(),
                                c.author.html_url.to_string(),
                                c.contributions,
                            ))?;
                        }

                        contributors.extend(first.items.clone());

                        if let Some(pages) = first.number_of_pages() {
                            for page in 2..=pages {
                                limit = gh_rate_limited(Some(limit), &github_client).await?;
                                let next =
                                    repo_handler.list_contributors().page(page).send().await?;
                                for c in next.items.iter() {
                                    contrib_sx.send((
                                        data.name.clone(),
                                        c.author.login.clone(),
                                        c.author.html_url.to_string(),
                                        c.contributions,
                                    ))?;
                                }
                                contributors.extend(next.items);
                            }
                        }

                        write_cached(&src, (data, contributors)).await;
                    } else {
                        eprintln!("failed to parse github url: {src}");
                    }
                }
            }

            anyhow::Ok(())
        }
    });

    let out_gl = tokio::spawn(async move {
        println!("{} other sources...", other_sources.len());

        for src in other_sources {
            if let Some((data, contributors)) =
                read_cached::<(GitLabProject, Vec<GitLabContributor>)>(&src).await
            {
                println!("cached data for: {src}");

                for c in contributors.iter() {
                    contrib_sx.send((
                        data.name.clone(),
                        c.name.clone(),
                        // TODO: get to user page url...
                        Default::default(),
                        c.commits,
                    ))?;
                }
            } else {
                let parsed = unformat!("https://{}/{}/{}", &src);
                if let Some((base, owner, repo)) = parsed {
                    // split-off any monorepo paths
                    let repo = repo.split("/").next().unwrap_or(repo);

                    let url = format!("https://{base}/api/v4/projects/{owner}%2F{repo}");
                    println!("fetching {base} data for: {owner}/{repo}");
                    let data = reqwest::get(&url).await?.json::<GitLabProject>().await?;
                    let url = format!("{url}/repository/contributors");
                    let contributors = reqwest::get(&url)
                        .await?
                        .json::<Vec<GitLabContributor>>()
                        .await?;
                    for c in contributors.iter() {
                        contrib_sx.send((
                            data.name.clone(),
                            c.name.clone(),
                            // TODO: get to user page url...
                            Default::default(),
                            c.commits,
                        ))?;
                    }
                    write_cached(&src, (data, contributors)).await;
                } else {
                    eprintln!("failed to parse gitlab url: {src}");
                }
            }
        }

        anyhow::Ok(())
    });

    let mut contributions = BTreeMap::new();

    while let Some((name, login, url, commits)) = contrib_rx.recv().await {
        let e = contributions.entry(name).or_insert(vec![]);
        if !login.ends_with("[bot]") {
            e.push((login, url, commits));
        }
    }

    _ = out_gh.await??;
    _ = out_gl.await??;

    println!("Got all data. generating...");

    let mut handlebars = Handlebars::new();
    handlebars.register_helper("plural", Box::new(plural_helper));

    if let Some(p) = args.template {
        let template = fs::read_to_string(p.as_path()).await?;
        handlebars.register_template_string("template", template.as_str())?;
    } else {
        handlebars.register_template_string("template", TEMPLATE)?;
    }

    let threshold = args.contributions_threshold;
    let data: TemplateData = match args.format {
        Format::NameAndCount => {
            let mut others = HashSet::new();
            let mut thank = Vec::from_iter(
                contributions
                    .into_iter()
                    .fold(HashMap::new(), |mut acc, (_, entries)| {
                        let sole = entries.len() == 1;

                        for (login, profile_url, commits) in entries {
                            if !sole && (commits as usize) < threshold {
                                _ = others.insert(login);
                                continue;
                            } else {
                                _ = others.remove(&login);
                            }

                            let entry =
                                acc.entry(login.clone()).or_insert(ThankData::NameAndCount {
                                    name: login,
                                    profile_url,
                                    count: 0,
                                });
                            match entry {
                                ThankData::NameAndCount { count, .. } => *count += commits as usize,
                                _ => unreachable!(),
                            }
                        }
                        acc
                    })
                    .into_values(),
            );

            thank.sort_by(|th_1, th_2| match (th_1, th_2) {
                (
                    ThankData::NameAndCount {
                        count: count_1,
                        name: name_1,
                        ..
                    },
                    ThankData::NameAndCount {
                        count: count_2,
                        name: name_2,
                        ..
                    },
                ) => {
                    let o = count_2.cmp(count_1);
                    match o {
                        std::cmp::Ordering::Equal => name_1.cmp(name_2),
                        std::cmp::Ordering::Less => o,
                        std::cmp::Ordering::Greater => o,
                    }
                }
                _ => unreachable!(),
            });

            TemplateData {
                thank,
                others: others.len(),
                mention: args.mention,
            }
        }
        Format::DepAndNames => {
            let mut others = HashSet::new();

            let thank = contributions
                .into_iter()
                .map(|(crate_name, contributors)| ThankData::DepAndNames {
                    crate_name,
                    contributors: {
                        let sole = contributors.len() == 1;

                        BTreeSet::from_iter(contributors.into_iter().filter_map(
                            |(login, url, commits)| {
                                if !sole && (commits as usize) < threshold {
                                    _ = others.insert(login);
                                    None
                                } else {
                                    _ = others.remove(&login);
                                    Some((login, url))
                                }
                            },
                        ))
                    },
                })
                .collect();
            TemplateData {
                thank,
                others: others.len(),
                mention: args.mention,
            }
        }
        Format::NameAndDeps => {
            let mut others = HashSet::new();

            let mut thank = Vec::from_iter(
                contributions
                    .into_iter()
                    .fold(HashMap::new(), |mut acc, (crate_name, entries)| {
                        let sole = entries.len() == 1;

                        for (login, profile_url, commits) in entries {
                            if !sole && (commits as usize) < threshold {
                                _ = others.insert(login);
                                continue;
                            } else {
                                _ = others.remove(&login);
                            }

                            let entry =
                                acc.entry(login.clone()).or_insert(ThankData::NameAndDeps {
                                    name: login,
                                    profile_url,
                                    crates: BTreeSet::new(),
                                });
                            match entry {
                                ThankData::NameAndDeps { crates, .. } => {
                                    _ = crates.insert(crate_name.clone());
                                }
                                _ => unreachable!(),
                            }
                        }
                        acc
                    })
                    .into_values(),
            );
            thank.sort_by(|th_1, th_2| match (th_1, th_2) {
                (
                    ThankData::NameAndDeps {
                        crates: crates_1,
                        name: name_1,
                        ..
                    },
                    ThankData::NameAndDeps {
                        crates: crates_2,
                        name: name_2,
                        ..
                    },
                ) => {
                    let o = crates_2.len().cmp(&crates_1.len());
                    match o {
                        std::cmp::Ordering::Equal => name_1.cmp(name_2),
                        std::cmp::Ordering::Less => o,
                        std::cmp::Ordering::Greater => o,
                    }
                }
                _ => unreachable!(),
            });
            TemplateData {
                thank,
                others: others.len(),
                mention: args.mention,
            }
        }
    };

    // println!("data: {}", serde_json::to_string(&data)?);

    let generated = handlebars.render("template", &data)?;

    let output_file_path = args.output.unwrap_or_else(|| {
        let mut path = args.path.clone();
        path.push(FILE_NAME);
        path
    });

    fs::write(output_file_path, generated).await?;

    Ok(())
}

async fn gh_rate_limited(
    limit: Option<RateLimit>,
    client: &octocrab::Octocrab,
) -> anyhow::Result<RateLimit> {
    let mut limit = match limit {
        Some(l) => l,
        None => client.ratelimit().get().await?,
    };

    if limit.resources.core.remaining > 0 {
        limit.resources.core.remaining -= 1;
        anyhow::Ok(limit)
    } else {
        let timeout =
            chrono::DateTime::<chrono::Utc>::from_timestamp(limit.resources.core.reset as i64, 0)
                .expect("create timeout");
        let now = chrono::Utc::now();
        let duration = timeout.signed_duration_since(now);
        let seconds = duration.num_seconds() as u64;
        for _ in 1..=seconds {
            let now = chrono::Utc::now();
            let duration = timeout.signed_duration_since(now);
            print!("\rHonouring your contributors {} requests were made, now please honour github's rate limit, and wait kindly {:0>2}m {:0>2}s...",
                limit.resources.core.limit,
                duration.num_minutes(),
                duration.num_seconds() - duration.num_minutes() * 60,
            );
            std::io::Write::flush(&mut std::io::stdout()).unwrap();

            sleep(Duration::from_secs(1)).await;
        }
        let mut new_limit = client.ratelimit().get().await?;
        new_limit.resources.core.limit += limit.resources.core.limit;
        anyhow::Ok(new_limit)
    }
}

fn plural_helper(
    h: &handlebars::Helper,
    _: &Handlebars,
    _: &handlebars::Context,
    _: &mut handlebars::RenderContext,
    out: &mut dyn handlebars::Output,
) -> handlebars::HelperResult {
    let count = h
        .param(0)
        .map(|p| p.value().as_number().map(|p| p.as_u64()))
        .flatten()
        .flatten()
        .ok_or(handlebars::RenderErrorReason::MissingVariable(Some(
            "expected count param".to_string(),
        )))?;

    let singular = h.param(1).map(|p| p.value().as_str()).flatten().ok_or(
        handlebars::RenderErrorReason::MissingVariable(Some("expected singular param".to_string())),
    )?;

    let plural = h.param(2).map(|p| p.value().as_str()).flatten().ok_or(
        handlebars::RenderErrorReason::MissingVariable(Some("expected plural param".to_string())),
    )?;

    if count == 1 {
        out.write(singular)?;
    } else {
        out.write(plural)?;
    }

    Ok(())
}

async fn read_cached<T>(key: &str) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    if let Some(dir) = dirs::cache_dir() {
        let mut path = dir.clone();
        path.push(CACHE_NAME);
        cacache::read(path, key)
            .await
            .map(|d: Vec<u8>| serde_json::from_slice::<T>(d.as_slice()).ok())
            .ok()
            .flatten()
    } else {
        None
    }
}

async fn write_cached<T>(key: &str, data: T)
where
    T: serde::ser::Serialize,
{
    if let Some(dir) = dirs::cache_dir() {
        let mut path = dir.clone();
        path.push(CACHE_NAME);

        if let Ok(data) = serde_json::to_vec(&data) {
            _ = cacache::write(path, key, data).await;
        }
    }
}

async fn clear_cache() -> anyhow::Result<()> {
    if let Some(dir) = dirs::cache_dir() {
        let mut path = dir.clone();
        path.push(CACHE_NAME);
        cacache::clear(path).await?;
    }
    anyhow::Ok(())
}

fn manifest_deps(path: &PathBuf, depth: &Breadth) -> anyhow::Result<Vec<(String, Dependency)>> {
    let manifest = Manifest::from_path(path.as_path()).or_else(|_| {
        let mut path = path.clone();
        path.push("Cargo.toml");
        Manifest::from_path(path.as_path())
    })?;

    let mut deps: Vec<_> = match depth {
        Breadth::NonOpt => manifest
            .dependencies
            .iter()
            .filter(|d| !d.1.optional())
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect(),
        Breadth::All => manifest
            .dependencies
            .iter()
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect(),
        Breadth::BuildAndDev => manifest
            .dependencies
            .iter()
            .chain(manifest.dev_dependencies.iter())
            .chain(manifest.build_dependencies.iter())
            .map(|(k, d)| (k.clone(), d.clone()))
            .collect(),
    };

    if let Some(workspace) = manifest.workspace {
        match depth {
            Breadth::BuildAndDev => deps.extend(
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
