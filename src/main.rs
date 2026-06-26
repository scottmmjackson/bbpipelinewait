use bbpipelinewait::{get_running_pipelines, init_config, list_running_pipelines, load_config, poll_pipeline, poll_pipelines};
use clap::{command, Arg, Command};
use std::process::exit;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
                        .required(true),
                )
                .arg(
                    Arg::new("repo")
                        .short('r')
                        .long("repo")
                        .value_name("repo")
                        .env("BITBUCKET_REPOSITORY")
                        .help("Bitbucket repository")
                        .required(true),
                ),
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
                        .required(true),
                )
                .arg(
                    Arg::new("repo")
                        .short('r')
                        .long("repo")
                        .value_name("repo")
                        .env("BITBUCKET_REPOSITORY")
                        .help("Bitbucket repository")
                        .required(true),
                )
                .arg(
                    Arg::new("pipeline-id")
                        .short('p')
                        .long("pipeline-id")
                        .value_name("pipeline-id")
                        .env("BITBUCKET_PIPELINE_ID")
                        .action(clap::ArgAction::Append)
                        .help(
                            "Bitbucket pipeline ID. May be repeated to wait for multiple \
                        pipelines. If left out, waits for all currently running pipelines.",
                        ),
                ),
        )
        .subcommand(Command::new("init").about("Initializes configuration file"));
    let matches = app.clone().get_matches();

    if matches.subcommand_matches("init").is_some() {
        init_config();
    }
    let config = load_config();

    match matches.subcommand_name() {
        Some("list") => {
            let list_command = matches.subcommand_matches("list").unwrap().clone();
            let workspace = list_command.get_one::<String>("workspace").unwrap();
            let repo = list_command.get_one::<String>("repo").unwrap();
            let pipelines = get_running_pipelines(&config, workspace, repo).await?;
            list_running_pipelines(&pipelines);
            exit(0);
        }
        Some("wait") => {
            let wait_command = matches.subcommand_matches("wait").unwrap().clone();
            let workspace = wait_command.get_one::<String>("workspace").unwrap();
            let repo = wait_command.get_one::<String>("repo").unwrap();
            let pipeline_ids: Vec<String> = wait_command
                .get_many::<String>("pipeline-id")
                .unwrap_or_default()
                .cloned()
                .collect();

            let ids_to_wait: Vec<String> = if pipeline_ids.is_empty() {
                let pipelines = get_running_pipelines(&config, workspace, repo).await?;
                if pipelines.is_empty() {
                    println!("ERROR: No running pipelines found.");
                    exit(1);
                }
                pipelines
                    .iter()
                    .map(|p| p.build_number.to_string())
                    .collect()
            } else {
                pipeline_ids
            };

            if ids_to_wait.len() == 1 {
                poll_pipeline(&config, workspace, repo, &ids_to_wait[0]).await?;
            } else {
                poll_pipelines(&config, workspace, repo, &ids_to_wait).await?;
            }
            exit(0);
        }
        _ => {
            let _ = app.print_long_help();
            println!();
            exit(1);
        }
    }
}
