use anyhow::Error;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use clap_verbosity_flag::{InfoLevel, Level as VerbosityLevel};
use reqwest::{Method, RequestBuilder};
use retronomicon_dto as dto;
use retronomicon_dto::types::IdOrSlug;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, info, Level};
use tracing_subscriber::fmt::Subscriber;
use url::Url;

#[derive(Debug, Parser)]
struct Opts {
    #[command(subcommand)]
    pub command: Command,

    /// Server to connect to.
    // In debug mode this is set to localhost:8000, while in production this is set to
    // retronomicon.com.
    #[clap(long, env = "RETRONOMICON_SERVER", hide_env_values = true)]
    #[cfg_attr(debug_assertions, clap(default_value = "http://localhost:8000/"))]
    #[cfg_attr(
        not(debug_assertions),
        clap(default_value = "https://retronomicon.com/")
    )]
    pub server: Url,

    /// A token to use for authentication.
    #[clap(long, env = "RETRONOMICON_TOKEN", hide_env_values = true)]
    pub token: Option<String>,

    /// Output pretty formatted JSON (no colors).
    #[clap(
        long,
        global = true,
        env = "RETRONOMICON_PRETTY",
        hide_env_values = true
    )]
    pub pretty: bool,

    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}

#[derive(Debug, Parser)]
enum Command {
    /// Core commands.
    Cores(CoreOpts),

    /// Platform commands.
    Platforms(PlatformOpts),

    /// System commands.
    Systems(SystemOpts),

    /// Team commands.
    Teams(TeamOpts),

    /// User specific commands.
    Users(UserOpts),

    /// Returns the authentication information.
    Whoami,
}

#[derive(Debug, Parser)]
pub struct CoreReleaseOpts {
    /// The core to refer for releases.
    #[clap(long)]
    core: String,

    #[command(subcommand)]
    pub command: ReleaseCommand,
}

#[derive(Debug, Parser)]
pub enum ReleaseCommand {
    /// List releases.
    List(ReleaseListOpts),

    /// Create a new release.
    Create(ReleaseCreateOpts),

    /// Get the details of a release.
    Get(ReleaseGetOpts),

    /// Download an artifact.
    Download(ReleaseDownloadOpts),

    /// List artifacts.
    Artifacts(ReleaseArtifactsOpts)
}

#[derive(Debug, Parser)]
pub struct ReleaseArtifactsOpts {
    /// The release's id.
    release_id: String,
}

#[derive(Debug, Parser)]
pub struct ReleaseDownloadOpts {
    /// The release's id.
    release_id: String,

    /// The artifact id.
    artifact: u32,
}

#[derive(Debug, Parser)]
pub struct ReleaseListOpts {
    #[clap(flatten)]
    paging: PagingParams,
}

#[derive(Debug, Parser)]
pub struct ReleaseCreateOpts {
    /// The platform to release.
    #[clap(long)]
    platform: String,

    /// The version of the release. This must be unique per core+platform.
    #[clap(long)]
    version: String,

    /// Release notes, in Markdown.
    #[clap(long)]
    notes: String,

    /// Date and time the release was made. By default will use the current timestamp.
    #[clap(long)]
    date_released: Option<String>,

    /// Whether this release is a prerelease. Prereleases are not shown by default.
    #[clap(long)]
    prerelease: bool,

    /// Release's links. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    links: Vec<String>,

    /// Release's metadata. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    metadata: Vec<String>,

    /// Release's files. These are going to be uploaded along with the release.
    #[clap(long)]
    files: Vec<PathBuf>,
}

#[derive(Debug, Parser)]
pub struct ReleaseGetOpts {
    /// The release's slug or numerical id.
    id: String,
}

#[derive(Debug, Parser)]
pub struct CoreOpts {
    #[command(subcommand)]
    pub command: CoreCommand,
}

#[derive(Debug, Parser)]
pub enum CoreCommand {
    /// Core-Release commands.
    Releases(CoreReleaseOpts),

    /// List cores.
    List(CoresListOpts),

    /// Create a new core.
    Create(CoreCreateOpts),

    /// Get the details of a core.
    Get(CoreGetOpts),

    /// Update a core.
    Update(CoreUpdateOpts),
}

#[derive(Debug, Parser)]
pub struct CoresListOpts {
    #[clap(flatten)]
    paging: PagingParams,
}

#[derive(Debug, Parser)]
pub struct CoreCreateOpts {
    /// The name of the core to create. Must be unique.
    name: String,

    /// The slug for the URL.
    #[clap(long)]
    slug: String,

    #[clap(long)]
    description: String,

    /// Either the system id or its slug.
    #[clap(long)]
    system: String,

    /// Either the system id or its slug.
    #[clap(long)]
    team: String,

    /// Core's links. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    links: Vec<String>,

    /// Core's metadata. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    metadata: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct CoreGetOpts {
    /// The core's slug or numerical id.
    id: String,
}

#[derive(Debug, Parser)]
pub struct CoreUpdateOpts {}

#[derive(Debug, Parser)]
pub struct TeamOpts {
    #[command(subcommand)]
    pub command: TeamCommand,
}

#[derive(Debug, Parser)]
pub enum TeamCommand {
    /// List teams.
    List(TeamsListOpts),

    /// Get a team's details.
    Get(TeamGet),

    /// Create a new team.
    Create(TeamCreateOpts),
}

#[derive(Debug, Parser)]
pub struct TeamsListOpts {
    #[clap(flatten)]
    paging: PagingParams,
}

#[derive(Debug, Parser)]
pub struct TeamGet {
    /// The team's name or numerical id.
    id: String,
}

#[derive(Debug, Parser)]
pub struct TeamCreateOpts {
    /// The name of the team to create. Must be unique.
    name: String,

    /// The slug for the URL.
    #[clap(long)]
    slug: String,

    #[clap(long, default_value = "")]
    description: String,

    /// Team's links. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    links: Vec<String>,

    /// Team's metadata. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    metadata: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct UserOpts {
    #[command(subcommand)]
    pub command: UserCommand,
}

#[derive(Debug, Parser)]
pub enum UserCommand {
    /// Update the user information.
    Update(UpdateUser),

    /// List users.
    List(UsersList),

    /// Get a user's details.
    Get(UserGet),
}

#[derive(Debug, Parser)]
pub struct PlatformOpts {
    #[command(subcommand)]
    pub command: PlatformCommand,
}

#[derive(Debug, Parser)]
pub enum PlatformCommand {
    /// List platforms.
    List(PlatformsListOpts),

    /// Create a platform.
    Create(PlatformCreateOpts),
}

#[derive(Debug, Parser)]
pub struct PlatformsListOpts {
    #[clap(flatten)]
    paging: PagingParams,
}

#[derive(Debug, Parser)]
pub struct PlatformCreateOpts {
    /// The name of the platform to create. Must be unique.
    name: String,

    /// The slug for the URL.
    #[clap(long)]
    slug: String,

    /// The team to own the new platform.
    #[clap(long)]
    team: String,

    #[clap(long)]
    description: String,

    /// Platform's links. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    links: Vec<String>,

    /// Platform's metadata. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    metadata: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct SystemOpts {
    #[command(subcommand)]
    pub command: SystemCommand,
}

#[derive(Debug, Parser)]
pub enum SystemCommand {
    /// List systems.
    List(SystemsListOpts),

    /// Create a new system.
    Create(SystemCreateOpts),

    /// Get the details of a system.
    Get(SystemGetOpts),
}

#[derive(Debug, Parser)]
pub struct SystemGetOpts {
    /// The system's slug or numerical id.
    id: String,
}

#[derive(Debug, Parser)]
pub struct SystemsListOpts {
    #[clap(flatten)]
    paging: PagingParams,
}

#[derive(Debug, Parser)]
pub struct SystemCreateOpts {
    /// The name of the system to create. Must be unique.
    name: String,

    /// The slug for the URL.
    #[clap(long)]
    slug: String,

    #[clap(long)]
    description: String,

    #[clap(long)]
    manufacturer: String,

    /// System's links. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    links: Vec<String>,

    /// System's metadata. This is a key-value pair, separated by an equal sign.
    #[clap(long)]
    metadata: Vec<String>,

    /// The team that owns the system. The user must be an admin of this team.
    /// Can be a slug or a numerical id.
    #[clap(long)]
    team: String,
}

#[derive(Debug, Parser)]
pub struct PagingParams {
    /// The page to download.
    #[clap(long)]
    page: Option<u32>,
    /// The maximum number of items to return.
    #[clap(long)]
    limit: Option<u32>,
}

impl PagingParams {
    pub fn to_query(&self) -> String {
        match (self.page, self.limit) {
            (None, None) => "".to_string(),
            (Some(page), None) => format!("page={page}"),
            (None, Some(limit)) => format!("limit={limit}"),
            (Some(page), Some(limit)) => format!("page={page}&limit={limit}"),
        }
    }
}

#[derive(Debug, Parser)]
pub struct UpdateUser {
    /// Who to update (defaults to the current user, can be ).
    pub user: Option<String>,

    /// The new username.
    #[clap(long)]
    pub username: Option<String>,

    /// The new user description.
    #[clap(long)]
    pub description: Option<String>,

    /// Add a link to the user's links. This is a key-value pair, separated by an equal sign.
    /// Can be repeated.
    #[clap(long)]
    pub add_link: Vec<String>,

    /// Remove a link to the user's links. This is the key to be removed. Can be repeated.
    #[clap(long)]
    pub remove_link: Vec<String>,
}

#[derive(Debug, Parser)]
pub struct UsersList {
    #[clap(flatten)]
    paging: PagingParams,
}

#[derive(Debug, Parser)]
pub struct UserGet {
    /// The user's name or numerical id.
    id: String,
}

fn output_json<J: Serialize>(value: J, opts: &Opts) -> Result<(), anyhow::Error> {
    println!(
        "{}",
        if opts.pretty {
            serde_json::to_string_pretty(&value)?
        } else {
            serde_json::to_string(&value)?
        }
    );
    Ok(())
}

fn update_request<B: Serialize>(
    mut request: RequestBuilder,
    opts: &Opts,
    body: Option<B>,
) -> RequestBuilder {
    if let Some(body) = body {
        request = request.json(&body);
    }

    if let Some(token) = &opts.token {
        request = request.header("Authorization", format!("Bearer {}", token));
    }

    request
}

fn links_dictionary_from_arg(arg: &Vec<String>) -> Option<BTreeMap<&str, &str>> {
    if arg.is_empty() {
        None
    } else {
        Some(
            arg.iter()
                .map(|x| x.split_once('=').unwrap_or((x.as_str(), "")))
                .collect(),
        )
    }
}

fn metadata_dictionary_from_arg(
    arg: &Vec<String>,
) -> Result<Option<BTreeMap<&str, Value>>, anyhow::Error> {
    if arg.is_empty() {
        Ok(None)
    } else {
        arg.iter()
            .map(|x| {
                let (key, value) = x.split_once('=').unwrap_or((x.as_str(), "null"));
                Ok((key, serde_json::from_str(value)?))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(Some)
    }
}

async fn send<Q, R>(
    method: reqwest::Method,
    path: &str,
    opts: &Opts,
    request: Q,
) -> Result<R, Error>
where
    Q: Serialize,
    R: for<'de> Deserialize<'de>,
{
    let client = reqwest::Client::new();
    let request = update_request(
        client.request(method, opts.server.join(path)?),
        opts,
        Some(request),
    )
    .build()?;

    let response = client.execute(request).await?;

    if response.status().is_success() {
        response.json().await.map_err(Into::into)
    } else {
        Err(Error::msg(format!(
            "Status code: {}\n{}",
            response.status(),
            response.text().await?
        )))
    }
}

async fn get<R>(path: &str, opts: &Opts) -> Result<R, Error>
where
    R: for<'de> Deserialize<'de>,
{
    send(reqwest::Method::GET, path, opts, ()).await
}

async fn post<Q, R>(path: &str, opts: &Opts, request: Q) -> Result<R, Error>
where
    Q: Serialize,
    R: for<'de> Deserialize<'de>,
{
    send(reqwest::Method::POST, path, opts, request).await
}

async fn put<Q, R>(path: &str, opts: &Opts, request: Q) -> Result<R, Error>
where
    Q: Serialize,
    R: for<'de> Deserialize<'de>,
{
    send(reqwest::Method::PUT, path, opts, request).await
}

async fn upload<R>(
    method: reqwest::Method,
    path: &str,
    opts: &Opts,
    file_path: &Path,
) -> Result<R, Error>
where
    R: for<'de> Deserialize<'de>,
{
    let file = tokio::fs::File::open(file_path).await?;

    let client = reqwest::Client::new();
    let request = update_request(
        client.request(method, opts.server.join(path)?),
        opts,
        Option::<()>::None,
    )
    .header(
        reqwest::header::CONTENT_DISPOSITION,
        format!(
            "attachment; filename=\"{}\"",
            file_path.file_name().unwrap().to_string_lossy()
        ),
    )
    .header(
        reqwest::header::CONTENT_TYPE,
        mime_guess2::from_ext(&file_path.extension().unwrap().to_string_lossy())
            .first_raw()
            .unwrap_or("application/octet-stream"),
    )
    .body(file)
    .build()?;

    let response = client.execute(request).await?;

    if response.status().is_success() {
        response.json().await.map_err(Into::into)
    } else {
        Err(Error::msg(format!(
            "Status code: {}\n{}",
            response.status(),
            response.text().await?
        )))
    }
}

async fn whoami(opts: &Opts) -> Result<(), anyhow::Error> {
    let response: dto::user::UserDetails = get("/api/v1/me", opts).await?;
    output_json(response, opts)
}

async fn user_update(
    opts: &Opts,
    _user_opts: &UserOpts,
    UpdateUser {
        user,
        username,
        description,
        add_link,
        remove_link,
    }: &UpdateUser,
) -> Result<(), anyhow::Error> {
    let url = if let Some(u) = user {
        format!("/api/v1/user/{u}")
    } else {
        "/api/v1/me/update".to_string()
    };

    let remove_links = if remove_link.is_empty() {
        None
    } else {
        Some(remove_link.iter().map(|x| x.as_str()).collect())
    };

    let response: dto::Ok = put(
        &url,
        opts,
        dto::user::UserUpdate {
            username: username.as_deref(),
            display_name: None,
            description: description.as_deref(),
            add_links: links_dictionary_from_arg(add_link),
            remove_links,
            ..Default::default()
        },
    )
    .await?;
    output_json(response, opts)
}

async fn release(opts: &Opts, release_opts: &CoreReleaseOpts) -> Result<(), anyhow::Error> {
    let core = IdOrSlug::parse(&release_opts.core);

    match &release_opts.command {
        ReleaseCommand::List(list_opts) => {
            let query = format!(
                "/api/v1/cores/{core}/releases?{}",
                list_opts.paging.to_query()
            );

            let response: Vec<dto::cores::releases::CoreReleaseListItem> =
                get(&query, opts).await?;
            output_json(response, opts)
        }
        ReleaseCommand::Create(create_opts) => {
            info!("Creating the release...");
            let date_released = match &create_opts.date_released {
                None => None,
                Some(x) => Some(
                    chrono::DateTime::parse_from_rfc3339(x)
                        .map(|d| d.naive_utc())
                        .or_else(|_| {
                            chrono::NaiveDate::parse_from_str(x, "%Y-%m-%d")
                                .map(|d| d.and_time(chrono::NaiveTime::default()))
                        })?
                        .timestamp(),
                ),
            };
            let response: dto::cores::releases::CoreReleaseCreateResponse = post(
                &format!("/api/v1/cores/{core}/releases"),
                opts,
                dto::cores::releases::CoreReleaseCreateRequest {
                    version: &create_opts.version,
                    notes: &create_opts.notes,
                    date_released,
                    prerelease: create_opts.prerelease,
                    links: links_dictionary_from_arg(&create_opts.links).unwrap_or_default(),
                    metadata: metadata_dictionary_from_arg(&create_opts.metadata)?
                        .unwrap_or_default(),
                    platform: IdOrSlug::parse(&create_opts.platform),
                },
            )
            .await?;

            output_json(&response, opts)?;

            let release_id = response.id;
            for path in &create_opts.files {
                info!("Uploading file '{path:?}'...");
                let _response: dto::Ok = upload(
                    Method::POST,
                    &format!("/api/v1/cores/{core}/releases/{release_id}/artifacts"),
                    opts,
                    path,
                )
                .await?;
            }
            info!("Done.");

            Ok(())
        }
        ReleaseCommand::Get(ReleaseGetOpts { id: _ }) => {
            todo!()
        }
        ReleaseCommand::Download(ReleaseDownloadOpts {
            release_id,
            artifact,
        }) => {
            let client = reqwest::Client::new();
            let request = update_request(
                client.get(opts.server.join(&format!(
                    "/api/v1/cores/{core}/releases/{release_id}/artifacts/{artifact}/download"
                ))?),
                opts,
                None::<()>,
            )
            .build()?;

            let response = client.execute(request).await?.bytes().await?.to_vec();
            std::io::stdout().write_all(&response)?;
            Ok(())
        }
        ReleaseCommand::Artifacts(ReleaseArtifactsOpts {
            release_id,
        }) => {
            let client = reqwest::Client::new();
            let request = update_request(
                client.get(opts.server.join(&format!(
                    "/api/v1/cores/{core}/releases/{release_id}/artifacts"
                ))?),
                opts,
                None::<()>,
            )
            .build()?;

            let response = client.execute(request).await?.bytes().await?.to_vec();
            std::io::stdout().write_all(&response)?;
            Ok(())
        }
    }
}

async fn core(opts: &Opts, core_opts: &CoreOpts) -> Result<(), anyhow::Error> {
    match &core_opts.command {
        CoreCommand::Releases(release_opts) => release(&opts, release_opts).await,

        CoreCommand::List(list_opts) => {
            let query = format!("/api/v1/cores?{}", list_opts.paging.to_query());

            let response: Vec<dto::cores::CoreListItem> = get(&query, opts).await?;
            output_json(response, opts)
        }
        CoreCommand::Create(create_opts) => {
            let response: dto::cores::CoreCreateResponse = post(
                "/api/v1/cores",
                opts,
                dto::cores::CoreCreateRequest {
                    name: &create_opts.name,
                    slug: &create_opts.slug,
                    description: &create_opts.description,
                    links: links_dictionary_from_arg(&create_opts.links).unwrap_or_default(),
                    metadata: metadata_dictionary_from_arg(&create_opts.metadata)?
                        .unwrap_or_default(),
                    system: IdOrSlug::parse(&create_opts.system),
                    owner_team: IdOrSlug::parse(&create_opts.team),
                },
            )
            .await?;
            output_json(response, opts)
        }
        CoreCommand::Get(CoreGetOpts { id }) => {
            let response: dto::cores::CoreDetailsResponse =
                get(&format!("/api/v1/cores/{id}"), opts).await?;
            output_json(response, opts)
        }
        CoreCommand::Update(CoreUpdateOpts {}) => {
            todo!()
        }
    }
}

async fn user(opts: &Opts, user_opts: &UserOpts) -> Result<(), anyhow::Error> {
    match &user_opts.command {
        UserCommand::Update(update_opts) => user_update(opts, user_opts, update_opts).await,
        UserCommand::List(UsersList { paging }) => {
            let query = format!("/api/v1/users?{}", paging.to_query());

            let response: Vec<dto::user::UserRef> = get(&query, opts).await?;
            output_json(response, opts)
        }
        UserCommand::Get(UserGet { id }) => {
            let response: dto::user::UserDetails =
                get(&format!("/api/v1/users/{}", id), opts).await?;
            output_json(response, opts)
        }
    }
}

async fn platform(opts: &Opts, platform_opts: &PlatformOpts) -> Result<(), anyhow::Error> {
    match &platform_opts.command {
        PlatformCommand::List(list_opts) => {
            let query = format!("/api/v1/platforms?{}", list_opts.paging.to_query());

            let response: Vec<dto::platforms::Platform> = get(&query, opts).await?;
            output_json(response, opts)
        }
        PlatformCommand::Create(create_opts) => {
            let response: dto::platforms::PlatformCreateResponse = post(
                "/api/v1/platforms",
                opts,
                dto::platforms::PlatformCreateRequest {
                    name: &create_opts.name,
                    slug: &create_opts.slug,
                    description: &create_opts.description,
                    links: links_dictionary_from_arg(&create_opts.links),
                    metadata: metadata_dictionary_from_arg(&create_opts.metadata)?,
                    owner_team: IdOrSlug::parse(&create_opts.team),
                },
            )
            .await?;
            output_json(response, opts)
        }
    }
}

async fn system(opts: &Opts, system_opts: &SystemOpts) -> Result<(), anyhow::Error> {
    match &system_opts.command {
        SystemCommand::List(list_opts) => {
            let query = format!("/api/v1/systems?{}", list_opts.paging.to_query());

            let response: Vec<dto::systems::SystemListItem> = get(&query, opts).await?;
            output_json(response, opts)
        }

        SystemCommand::Create(SystemCreateOpts {
            name,
            slug,
            description,
            manufacturer,
            links,
            metadata,
            team,
        }) => {
            let response: dto::systems::SystemCreateResponse = post(
                "/api/v1/systems",
                opts,
                dto::systems::SystemCreateRequest {
                    name,
                    slug,
                    description,
                    manufacturer,
                    links: links_dictionary_from_arg(links),
                    metadata: metadata_dictionary_from_arg(metadata)?,
                    owner_team: IdOrSlug::parse(team),
                },
            )
            .await?;
            output_json(response, opts)
        }

        SystemCommand::Get(SystemGetOpts { id }) => {
            let response: dto::systems::SystemDetails =
                get(&format!("/api/v1/systems/{}", id), opts).await?;
            output_json(response, opts)
        }
    }
}

async fn team(opts: &Opts, team_opts: &TeamOpts) -> Result<(), anyhow::Error> {
    match &team_opts.command {
        TeamCommand::List(list_opts) => {
            let query = format!("/api/v1/teams?{}", list_opts.paging.to_query());
            let response: Vec<dto::teams::TeamRef> = get(&query, opts).await?;
            output_json(response, opts)
        }
        TeamCommand::Get(get_opts) => {
            let response: dto::teams::TeamDetails =
                get(&format!("/api/v1/teams/{}", get_opts.id), opts).await?;
            output_json(response, opts)
        }
        TeamCommand::Create(TeamCreateOpts {
            name,
            slug,
            description,
            links,
            metadata,
        }) => {
            let response: dto::teams::TeamCreateResponse = post(
                "/api/v1/teams",
                opts,
                dto::teams::TeamCreateRequest {
                    name,
                    slug,
                    description,
                    links: links_dictionary_from_arg(links),
                    metadata: metadata_dictionary_from_arg(metadata)?,
                },
            )
            .await?;
            output_json(response, opts)
        }
    }
}

#[tokio::main]
async fn main() {
    let opts = Opts::parse();
    debug!(?opts);

    // Initialize tracing.
    let subscriber = Subscriber::builder();
    let subscriber = match opts.verbose.log_level() {
        Some(VerbosityLevel::Error) => subscriber.with_max_level(Level::ERROR),
        Some(VerbosityLevel::Warn) => subscriber.with_max_level(Level::WARN),
        Some(VerbosityLevel::Info) => subscriber.with_max_level(Level::INFO),
        Some(VerbosityLevel::Debug) => subscriber.with_max_level(Level::DEBUG),
        None | Some(VerbosityLevel::Trace) => subscriber.with_max_level(Level::TRACE),
    };
    subscriber
        .with_ansi(true)
        .with_writer(std::io::stderr)
        .init();

    let result = match &opts.command {
        Command::Platforms(platform_opts) => platform(&opts, platform_opts).await,
        Command::Systems(system_opts) => system(&opts, system_opts).await,
        Command::Teams(team_opts) => team(&opts, team_opts).await,
        Command::Users(user_opts) => user(&opts, user_opts).await,
        Command::Whoami => whoami(&opts).await,
        Command::Cores(core_opts) => core(&opts, core_opts).await,
    };

    match result {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
