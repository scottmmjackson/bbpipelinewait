use clap::{Arg, ArgMatches, command, Command};
use directories::ProjectDirs;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::ops::Add;
use base64::{engine, Engine};
use reqwest::{Url};
use serde::de::DeserializeOwned;
use spinner::{SpinnerBuilder};
use strum_macros::{Display, EnumString};
#[allow(unused_imports)]
use strum;
use termsize;
use termsize::Size;

const SPINNER: [&str; 4] = ["▖", "▘","▝","▗"];

#[derive(Debug, Serialize, Deserialize)]
struct ConfigFile {
    username: String,
    app_password: String, // or oauth_token, depending on your authentication method
}

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, EnumString, Deserialize, Display, Clone)]
enum PipelineStates {
    SUCCESSFUL,
    COMPLETED,
    FAILED,
    IN_PROGRESS,
    PAUSED,
    HALTED,
    PENDING,
    BUILDING
}

// Derived from inspecting the pipelines filters in the Bitbucket UI
const IN_PROGRESS_STATES_QUERY: &str = "status=PENDING&status=BUILDING&status=IN_PROGRESS&fields=\
%2Bvalues.target.commit.message";

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, EnumString, Deserialize, Display, Clone)]
enum PipelineStages {
    PAUSED,
    RUNNING,
    PENDING
}

#[allow(non_camel_case_types)]
#[derive(Debug, PartialEq, EnumString, Deserialize, Display, Clone)]
enum PipelineResults {
    FAILED
}

#[derive(Debug, Deserialize, Clone)]
struct PipelineStage {
    name: PipelineStages,
}

#[derive(Debug, Deserialize, Clone)]
struct PipelineResult {
    name: PipelineResults
}

#[derive(Debug, Deserialize, Clone)]
struct PipelineState {
    name: PipelineStates,
    stage: Option<PipelineStage>,
    result: Option<PipelineResult>
}

#[derive(Debug, Deserialize, Clone)]
struct PipelineCreator {
    display_name: String
}

#[derive(Debug, Deserialize, Clone)]
struct PipelineCommit {
    message: String,
}

#[derive(Debug, Deserialize, Clone)]
struct PipelineTarget {
    ref_name: String,
    commit: PipelineCommit
}

#[derive(Debug, Deserialize, Clone)]
struct Pipeline {
    uuid: String,
    build_number: u32,
    state: PipelineState,
    creator: PipelineCreator,
    target: PipelineTarget
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
struct PipelinesResponse {
    values: Vec<Pipeline>,
    size: u32,
    pagelen: u32,
    page: u32,
    // https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pipelines/#api-repositories-workspace-repo-slug-pipelines-get
    // these are documented but do not exist in reality :upside_down_face:
    // next: Option<String>,
    // previous: Option<String>
}

#[allow(non_camel_case_types)]
#[derive(Debug, Deserialize, Clone)]
struct BitbucketErrorObject {
    message: String,
    detail: String,
}

#[derive(Debug, Deserialize, Clone)]
struct BitbucketError {
    error: BitbucketErrorObject
}

fn main() {
    let mut app = command!()
        .subcommand(
            Command::new("list")
                .about("Lists all running bitbucket pipelines.")
                .arg(
                    Arg::new("workspace")
                        .short('w')
                        .long("workspace")
                        .value_name("workspace")
                        .env("BITBUCKET_WORKSPACE")
                        .help("Bitbucket workspace")
                        .required(true))
                .arg(
                    Arg::new("repo")
                        .short('r')
                        .long("repo")
                        .value_name("repo")
                        .env("BITBUCKET_REPOSITORY")
                        .help("Bitbucket repository")
                        .required(true))
        )
        .subcommand(
            Command::new("wait")
                .about("Waits for the pipeline with the specified id to complete.")
                .arg(
                    Arg::new("workspace")
                        .short('w')
                        .long("workspace")
                        .value_name("workspace")
                        .env("BITBUCKET_WORKSPACE")
                        .help("Bitbucket workspace")
                        .required(true))
                .arg(
                    Arg::new("repo")
                        .short('r')
                        .long("repo")
                        .value_name("repo")
                        .env("BITBUCKET_REPOSITORY")
                        .help("Bitbucket repository")
                        .required(true))
                .arg(
                    Arg::new("pipeline-id")
                        .short('p')
                        .long("pipeline-id")
                        .value_name("pipeline-id")
                        .env("BITBUCKET_PIPELINE_ID")
                        .help("Bitbucket pipeline ID. If left out, we'll attempt to list all \
                        pipelines and select the only running one, if any."))
        )
        .subcommand(
            Command::new("init")
                .about("Initializes configuration file")
        );
    let matches = app.clone().get_matches();

    if let Some(_) = matches.subcommand_matches("init") {
        init_config();
    }
    let config = load_config();


    match matches.subcommand_name() {
        Some("list") => {
            let mut list_command = matches.subcommand_matches("list").unwrap().clone();
            let workspace = list_command.get_one::<String>("workspace").unwrap();
            let repo = list_command.get_one::<String>("repo").unwrap();
            let pipelines = get_running_pipelines(
                &config,
                workspace,
                repo
            );
            list_running_pipelines(pipelines);
        },
        Some("wait") => {
            let wait_command = matches.subcommand_matches("wait").unwrap().clone();
            let workspace = wait_command.get_one::<String>("workspace").unwrap();
            let repo = wait_command.get_one::<String>("repo").unwrap();
            let pipeline_id = wait_command.get_one::<String>("pipeline-id");
            if let Some(id) = pipeline_id {
                poll_pipeline(
                    &config,
                    workspace,
                    repo,
                    id
                );
                std::process::exit(1);
            } else {
                let pipelines = get_running_pipelines(&config, &workspace, &repo);
                if pipelines.len() != 1 {
                    println!("ERROR: Pipeline ID can only be elided if there's only one running \
                        pipeline. Listing running pipelines:");
                    list_running_pipelines(pipelines);
                    std::process::exit(1);
                } else {
                    let build_number = pipelines.get(0).unwrap().build_number.to_string();
                    let id = build_number.as_str();
                    poll_pipeline(
                        &config,
                        workspace,
                        repo,
                        id
                    );
                }
            }

        },
        _ => {
            let _ = app.print_long_help();
            println!();
            std::process::exit(1);
        }
    }
}

fn fetch_handler<T: DeserializeOwned>(client: &Client, url: &Url, fetch_type: &str) -> T {
    match client.get(url.as_str()).send() {
        Ok(response) => {
            let response_text = response.text().unwrap();
            match serde_json::from_str(response_text.as_str()) {
                Ok(t) => t,
                Err(e) => {
                    let bitbucket_error: Result<BitbucketError, serde_json::Error> =
                        serde_json::from_str(response_text.as_str());
                    if bitbucket_error.is_ok() {
                        let error = bitbucket_error.unwrap().error;
                        eprintln!("Error response for {}: {} - {}",
                        fetch_type, error.message, error.detail);
                        std::process::exit(1);
                    }
                    eprintln!("Error parsing response for {}: {}\nResponse text: {}",
                              fetch_type, e, response_text);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error fetching {}: {}", fetch_type, e);
            std::process::exit(1);
        }
    }
}

fn get_pipelines_responses(url: Url, client: Client) -> Vec<Pipeline>{
    let root: PipelinesResponse = fetch_handler(&client, &url, "pipeline list");
    if root.page*root.pagelen > root.size {
        Vec::from(root.values)
    } else {
        let mut cur = root.clone();
        let mut ret = Vec::from(root.values);
        while cur.page*cur.pagelen < cur.size {
            let mut next_url = Url::parse(url.as_str()).unwrap();
            next_url.set_query(Some(format!(
                "{}&page={}&pagelen={}",IN_PROGRESS_STATES_QUERY, cur.page+1, cur.pagelen
            ).as_str()));
            cur = fetch_handler(&client, &next_url, "pipeline list");
            ret.append(&mut cur.values);
            println!("Fetched page {}", cur.page);
        }
        ret
    }
}

fn init_config() {
    println!("Initializing config file.");
    let project_dir = ProjectDirs::from
        ("com", "scottmmjackson", "bbpipelinewait")
        .expect("Failed to get config directory.");
    let config_path = project_dir.config_dir();
    if !config_path.exists() {
        match fs::create_dir_all(&config_path) {
            Err(e) => {
                eprintln!("Error creating '{}': {}", config_path.to_str().unwrap(), e)
            }
            Ok(_) => {}
        };
    }
    let config_file = config_path.join("config.json");
    let dummy_config = ConfigFile {
        username: "my_username".to_string(),
        app_password: "my_app_password".to_string(),
    };
    if config_file.exists() {
        eprintln!("'{}' already exists, not creating it.", config_file.to_str().unwrap());
        std::process::exit(1);
    }
    fs::write(config_file, serde_json::to_string(&dummy_config).unwrap()).unwrap();
    std::process::exit(0);
}

fn load_config() -> ConfigFile {
    let config_path = ProjectDirs::from
        ("com", "scottmmjackson", "bbpipelinewait")
        .expect("Failed to open config directory.").config_dir().join("config.json");

    if let Ok(contents) = fs::read_to_string(&config_path) {
        serde_json::from_str(&contents).expect("Failed to parse config file")
    } else {
        eprintln!(
            "Config file not found at {:?}. Please create it with the required authentication details (username, app_password).",
            &config_path
        );
        std::process::exit(1);
    }
}

fn get_running_pipelines(config: &ConfigFile, workspace: &str, repo: &str) -> Vec<Pipeline> {
    let client = create_authenticated_client(config);
    let url = Url::parse(format!(
        "https://api.bitbucket.org/2.0/repositories/{}/{}/pipelines/?{}",
        workspace, repo, IN_PROGRESS_STATES_QUERY
    ).as_str()).unwrap();

    get_pipelines_responses(url, client)
}

fn list_running_pipelines(pipelines: Vec<Pipeline>) {
    let num_in_progress = pipelines.iter()
        .map(|pipeline| {
            let commit_message = pipeline.target.commit.message
                .replace("\n","");
            println!("Build Number: {build_number} Author: {author}, Branch: {branch}, \
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
    println!("{} pipelines, {} in progress.", pipelines.iter().count(), num_in_progress)
}

fn poll_pipeline(config: &ConfigFile, workspace: &str, repo: &str, pipeline_id: &str) {
    let client = create_authenticated_client(config);
    let url = Url::parse(format!(
        "https://api.bitbucket.org/2.0/repositories/{}/{}/pipelines/{}/?fields=%2Btarget.commit.message",
        workspace, repo, pipeline_id
    ).as_str()).unwrap();

    let sp = SpinnerBuilder::new("Fetching pipeline status...".into())
        .spinner(SPINNER.to_vec()).start();

    loop {
        let response: Pipeline = fetch_handler(
            &client, &url, format!("pipeline {}", pipeline_id).as_str());

        let stage: String = match response.state.stage {
            None => {
                if response.state.result.is_none() {
                    "Unknown Stage".to_string()
                } else {
                    response.state.result.unwrap().name.to_string()
                }
            }
            Some(stage) => {
                stage.name.to_string()
            }
        };
        let commit_message = response.target.commit.message
            .replace("\n","");
        let mut polling_message =
            format!("Build Number: {build_number}, Author: {author}, Branch: {branch}, \
            Commit Message: {commit_message} State: {state}, Stage: {stage}",
                                      build_number = response.build_number,
                                      author = response.creator.display_name,
                                      branch = response.target.ref_name,
                                      commit_message = commit_message,
                                      state = response.state.name);

        // Truncate to the width of the terminal minus the spinner.
        let default_size = Size { rows: 0, cols: 40 };
        let terminal_size = termsize::get().unwrap_or(default_size);
        if polling_message.len() > (terminal_size.cols - 2) as usize {
            polling_message.truncate(terminal_size.cols as usize-5);
            polling_message = polling_message.add("...");
        }
        sp.update(polling_message);

        if response.state.name != PipelineStates::IN_PROGRESS ||
            stage == PipelineStages::PAUSED.to_string().as_str() {
            break;
        }

        // Wait for some time before polling again (e.g., 10 seconds)
        std::thread::sleep(std::time::Duration::from_secs(10));
    }
}

fn create_authenticated_client(config: &ConfigFile) -> Client {
    let bearer_token = engine::general_purpose::STANDARD.encode(
        format!("{}:{}", config.username, config.app_password));
    let auth_header = format!(
        "Basic {}",
        bearer_token
    );

    let mut header_map = reqwest::header::HeaderMap::new();
    header_map.insert(reqwest::header::AUTHORIZATION, auth_header.parse().unwrap());

    Client::builder()
        .default_headers(header_map)
        .build()
        .unwrap()
}
