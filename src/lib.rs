use std::error::Error;
use base64::{engine, Engine};
use directories::ProjectDirs;
use reqwest::{Client, Url};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use spinner::SpinnerBuilder;
use std::fs;
use std::ops::Add;
use std::process::exit;
use std::time::Duration;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
#[allow(unused_imports)]
use strum;
use strum_macros::{Display, EnumString};
use termsize::Size;
use tokio::time;

pub const SPINNER: [&str; 4] = ["▖", "▘", "▝", "▗"];

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigFile {
    // Your Atlassian account email, or the literal string "x-bitbucket-api-token-auth".
    pub username: String,
    // A Bitbucket-scoped Atlassian API token (app passwords are deprecated). Create one at
    // id.atlassian.com via "Create API token with scopes", selecting Bitbucket as the app,
    // with the read:pipeline:bitbucket and read:repository:bitbucket scopes.
    pub api_token: String, // pragma: allowlist-secret
    // Optional shell command bbpw-ui offers to run on completion by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_on_complete: Option<String>,
}

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, EnumString, Deserialize, Display, Clone)]
pub enum PipelineStates {
    SUCCESSFUL,
    COMPLETED,
    FAILED,
    IN_PROGRESS,
    PAUSED,
    HALTED,
    PENDING,
    BUILDING,
}

// Derived from inspecting the pipelines filters in the Bitbucket UI
pub const IN_PROGRESS_STATES_QUERY: &str = "status=PENDING&status=BUILDING&status=IN_PROGRESS&fields=\
%2Bvalues.target.commit.message";

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, EnumString, Deserialize, Display, Clone)]
pub enum PipelineStages {
    PAUSED,
    RUNNING,
    PENDING,
}

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, EnumString, Deserialize, Display, Clone)]
pub enum PipelineResults {
    SUCCESSFUL,
    FAILED,
    ERROR,
    STOPPED,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PipelineStage {
    pub name: PipelineStages,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PipelineResult {
    pub name: PipelineResults,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PipelineState {
    pub name: PipelineStates,
    pub stage: Option<PipelineStage>,
    pub result: Option<PipelineResult>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PipelineCreator {
    pub display_name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PipelineCommit {
    pub message: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PipelineTarget {
    #[serde(alias = "source")]
    pub ref_name: String,
    pub commit: PipelineCommit,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Pipeline {
    pub uuid: String,
    pub build_number: u32,
    pub state: PipelineState,
    pub creator: PipelineCreator,
    pub target: PipelineTarget,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct PipelinesResponse {
    pub values: Vec<Pipeline>,
    pub size: u32,
    pub pagelen: u32,
    pub page: u32,
    // https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pipelines/#api-repositories-workspace-repo-slug-pipelines-get
    // these are documented but do not exist in reality :upside_down_face:
    // next: Option<String>,
    // previous: Option<String>
}

#[allow(non_camel_case_types)]
#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketErrorObject {
    pub message: String,
    pub detail: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BitbucketError {
    pub error: BitbucketErrorObject,
}

pub async fn fetch_handler<T: DeserializeOwned>(
    client: &ClientWithMiddleware,
    url: &Url,
    fetch_type: &str,
) -> Result<T, Box<dyn Error>> {
    let response = client.get(url.as_str()).send().await?;
    let status = response.status();
    let response_text = response.text().await?;

    if status == reqwest::StatusCode::UNAUTHORIZED {
        eprintln!(
            "Authentication failed (401) for {}. Bitbucket app passwords are deprecated; \
            `api_token` in your config must be a Bitbucket-scoped Atlassian API token, not an \
            old app password. Create one at id.atlassian.com via \"Create API token with \
            scopes\" (select Bitbucket as the app, with read:pipeline:bitbucket and \
            read:repository:bitbucket scopes), and set `username` to your Atlassian account \
            email or the literal string \"x-bitbucket-api-token-auth\".",
            fetch_type
        );
        exit(1);
    }

    match serde_json::from_str::<T>(&response_text) {
        Ok(t) => Ok(t),
        Err(e) => {
            if let Ok(error) = serde_json::from_str::<BitbucketError>(&response_text) {
                eprintln!(
                    "Error response for {}: {} - {}",
                    fetch_type, error.error.message, error.error.detail
                );
                exit(1);
            }
            eprintln!(
                "Error parsing response for {}: {}\nURL: {}\nStatus: {}\nResponse text: {:?}",
                fetch_type, e, url, status, response_text
            );
            exit(1);
        }
    }
}

pub async fn get_pipelines_responses(
    url: Url,
    client: ClientWithMiddleware,
) -> Result<Vec<Pipeline>, Box<dyn Error>> {
    let root: PipelinesResponse = fetch_handler(&client, &url, "pipeline list").await?;
    if root.page * root.pagelen > root.size {
        Ok(Vec::from(root.values))
    } else {
        let mut cur = root.clone();
        let mut ret = Vec::from(root.values);
        while cur.page * cur.pagelen < cur.size {
            let mut next_url = Url::parse(url.as_str()).unwrap();
            next_url.set_query(Some(
                format!(
                    "{}&page={}&pagelen={}",
                    IN_PROGRESS_STATES_QUERY,
                    cur.page + 1,
                    cur.pagelen
                )
                .as_str(),
            ));
            cur = fetch_handler(&client, &next_url, "pipeline list").await?;
            ret.append(&mut cur.values);
            println!("Fetched page {}", cur.page);
        }
        Ok(ret)
    }
}

pub fn init_config() {
    println!("Initializing config file.");
    let project_dir = ProjectDirs::from("com", "scottmmjackson", "bbpipelinewait")
        .expect("Failed to get config directory.");
    let config_path = project_dir.config_dir();
    if !config_path.exists() {
        match fs::create_dir_all(config_path) {
            Err(e) => {
                eprintln!("Error creating '{}': {}", config_path.to_str().unwrap(), e)
            }
            Ok(_) => {}
        };
    }
    let config_file = config_path.join("config.json");
    let dummy_config = ConfigFile {
        username: "my_username_or_email".to_string(),
        api_token: "my_api_token".to_string(), // pragma: allowlist-secret
        default_on_complete: None,
    };
    if config_file.exists() {
        eprintln!(
            "'{}' already exists, not creating it.",
            config_file.to_str().unwrap()
        );
        exit(1);
    }
    fs::write(config_file, serde_json::to_string(&dummy_config).unwrap()).unwrap();
    exit(0);
}

pub fn load_config() -> ConfigFile {
    let config_path = ProjectDirs::from("com", "scottmmjackson", "bbpipelinewait")
        .expect("Failed to open config directory.")
        .config_dir()
        .join("config.json");

    if let Ok(contents) = fs::read_to_string(&config_path) {
        serde_json::from_str(&contents).expect("Failed to parse config file")
    } else {
        eprintln!(
            "Config file not found at {:?}. Please create it with the required authentication details (username, api_token).",
            &config_path
        );
        exit(1);
    }
}

pub async fn get_running_pipelines(
    config: &ConfigFile,
    workspace: &str,
    repo: &str,
) -> Result<Vec<Pipeline>, Box<dyn Error>> {
    let client = create_authenticated_client(config);
    let url = Url::parse(
        format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/pipelines/?{}",
            workspace, repo, IN_PROGRESS_STATES_QUERY
        )
        .as_str(),
    )
    .unwrap();

    get_pipelines_responses(url, client).await
}

pub fn list_running_pipelines(pipelines: &[Pipeline]) {
    let num_in_progress = pipelines
        .iter()
        .map(|pipeline| {
            let commit_message = pipeline.target.commit.message.replace("\n", "");
            println!(
                "Build Number: {build_number} Author: {author}, Branch: {branch}, \
            Commit Message: {commit_message} State: {state}",
                build_number = pipeline.build_number,
                author = pipeline.creator.display_name,
                branch = pipeline.target.ref_name,
                commit_message = commit_message,
                state = pipeline.state.name
            );
            pipeline
        })
        .count();
    println!(
        "{} pipelines, {} in progress.",
        pipelines.iter().count(),
        num_in_progress
    )
}

pub async fn poll_pipelines(
    config: &ConfigFile,
    workspace: &str,
    repo: &str,
    pipeline_ids: &[String],
) -> Result<Vec<Pipeline>, Box<dyn Error>> {
    let client = create_authenticated_client(config);
    let mut remaining: Vec<String> = pipeline_ids.to_vec();
    let mut completed: Vec<Pipeline> = Vec::new();
    let mut interval = time::interval(Duration::from_secs(10));

    while !remaining.is_empty() {
        let mut next_remaining = Vec::new();
        for pipeline_id in &remaining {
            let url = Url::parse(&format!(
                "https://api.bitbucket.org/2.0/repositories/{}/{}/pipelines/{}/?fields=%2Btarget.commit.message",
                workspace, repo, pipeline_id
            ))
            .unwrap();
            let response: Pipeline =
                fetch_handler(&client, &url, &format!("pipeline {}", pipeline_id)).await?;
            let state_snap = response.state.clone();
            let stage: String = match state_snap.stage {
                None => state_snap
                    .result
                    .map_or("Unknown Stage".to_string(), |r| r.name.to_string()),
                Some(s) => s.name.to_string(),
            };
            let commit_message = response.target.commit.message.replace("\n", "");
            println!(
                "[#{build_number}] {author} | {branch} | {commit_message} | {state} | {stage}",
                build_number = response.build_number,
                author = response.creator.display_name,
                branch = response.target.ref_name,
                commit_message = commit_message,
                state = response.state.name,
            );
            let done = response.state.name != PipelineStates::IN_PROGRESS
                || stage == PipelineStages::PAUSED.to_string();
            if done {
                completed.push(response);
            } else {
                next_remaining.push(pipeline_id.clone());
            }
        }
        remaining = next_remaining;
        if !remaining.is_empty() {
            interval.tick().await;
        }
    }

    Ok(completed)
}

pub async fn poll_pipeline(
    config: &ConfigFile,
    workspace: &str,
    repo: &str,
    pipeline_id: &str,
) -> Result<Pipeline, Box<dyn Error>> {
    let client = create_authenticated_client(config);
    let url = Url::parse(
        format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/pipelines/{}/?fields=%2Btarget.commit.message",
            workspace, repo, pipeline_id
        )
        .as_str(),
    )
    .unwrap();

    let sp = SpinnerBuilder::new("Fetching pipeline status...".into())
        .spinner(SPINNER.to_vec())
        .start();

    let mut interval = time::interval(Duration::from_secs(10));

    let final_pipeline = loop {
        let response: Pipeline =
            fetch_handler(&client, &url, format!("pipeline {}", pipeline_id).as_str()).await?;

        // Clone state for display to avoid partial moves that would prevent storing response
        let state_snap = response.state.clone();
        let stage: String = match state_snap.stage {
            None => {
                if state_snap.result.is_none() {
                    "Unknown Stage".to_string()
                } else {
                    state_snap.result.unwrap().name.to_string()
                }
            }
            Some(stage) => stage.name.to_string(),
        };

        let commit_message = response.target.commit.message.replace("\n", "");
        let mut polling_message = format!(
            "Build Number: {build_number}, Author: {author}, Branch: {branch}, \
            Commit Message: {commit_message} State: {state}, Stage: {stage}",
            build_number = response.build_number,
            author = response.creator.display_name,
            branch = response.target.ref_name,
            commit_message = commit_message,
            state = response.state.name
        );

        // Truncate to the width of the terminal minus the spinner.
        let default_size = Size { rows: 0, cols: 40 };
        let terminal_size = termsize::get().unwrap_or(default_size);
        if polling_message.len() > (terminal_size.cols - 2) as usize {
            polling_message.truncate(terminal_size.cols as usize - 5);
            polling_message = polling_message.add("...");
        }
        sp.update(polling_message);

        let done = response.state.name != PipelineStates::IN_PROGRESS
            || stage == PipelineStages::PAUSED.to_string();

        if done {
            break response;
        }

        interval.tick().await;
    };

    drop(sp);
    Ok(final_pipeline)
}

pub fn create_authenticated_client(config: &ConfigFile) -> ClientWithMiddleware {
    let bearer_token = engine::general_purpose::STANDARD
        .encode(format!("{}:{}", config.username, config.api_token));
    let auth_header = format!("Basic {}", bearer_token);

    let mut header_map = reqwest::header::HeaderMap::new();
    header_map.insert(reqwest::header::AUTHORIZATION, auth_header.parse().unwrap());
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);

    ClientBuilder::new(Client::builder().default_headers(header_map).build().unwrap())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}
