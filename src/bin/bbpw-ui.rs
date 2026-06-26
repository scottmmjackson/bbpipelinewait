use bbpipelinewait::{get_running_pipelines, init_config, load_config, poll_pipeline, Pipeline};
use clap::{command, Arg, Command};
use inquire::{Confirm, Select, Text};
use std::error::Error;
use std::process::{exit, Command as ShellCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = command!()
        .name("bbpw-ui")
        .about("Interactive UI for waiting on Bitbucket pipelines with completion actions.")
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
                .help("Pipeline ID to wait for. If omitted, presents an interactive list."),
        )
        .subcommand(Command::new("init").about("Initializes the configuration file"))
        .get_matches();

    if matches.subcommand_matches("init").is_some() {
        init_config();
    }

    let config = load_config();
    let workspace = matches.get_one::<String>("workspace").unwrap();
    let repo = matches.get_one::<String>("repo").unwrap();

    let pipeline_id = if let Some(id) = matches.get_one::<String>("pipeline-id") {
        id.clone()
    } else {
        let pipelines = get_running_pipelines(&config, workspace, repo).await?;
        if pipelines.is_empty() {
            eprintln!("No running pipelines found.");
            exit(1);
        }
        let options: Vec<String> = pipelines.iter().map(pipeline_label).collect();
        let selected = Select::new("Select a pipeline to wait for:", options.clone())
            .prompt()
            .unwrap_or_else(|e| {
                eprintln!("Interactive prompt failed: {}. Use --pipeline-id to specify.", e);
                exit(1);
            });
        let idx = options.iter().position(|x| *x == selected).unwrap();
        pipelines[idx].build_number.to_string()
    };

    let notify = Confirm::new("Send a desktop notification on completion?")
        .with_default(false)
        .prompt()
        .unwrap_or(false);

    let on_complete_cmd = Text::new("Run a command on completion? (leave blank to skip)")
        .prompt()
        .unwrap_or_default();
    let on_complete_cmd = on_complete_cmd.trim().to_string();

    let final_pipeline = poll_pipeline(&config, workspace, repo, &pipeline_id).await?;
    let final_state = final_pipeline.state.name.to_string();

    println!(
        "\nPipeline #{} finished: {}",
        final_pipeline.build_number, final_state
    );

    if notify {
        send_notification(
            "Bitbucket Pipeline Complete",
            &format!("Pipeline #{} — {}", final_pipeline.build_number, final_state),
        );
    }

    if !on_complete_cmd.is_empty() {
        ShellCommand::new("sh")
            .arg("-c")
            .arg(&on_complete_cmd)
            .env("PIPELINE_STATE", &final_state)
            .env("PIPELINE_BUILD_NUMBER", final_pipeline.build_number.to_string())
            .status()?;
    }

    Ok(())
}

fn pipeline_label(p: &Pipeline) -> String {
    let commit = p.target.commit.message.replace('\n', " ");
    let commit_preview: String = commit.chars().take(50).collect();
    format!(
        "#{} | {} | {} | {}",
        p.build_number, p.target.ref_name, p.creator.display_name, commit_preview
    )
}

#[cfg(target_os = "macos")]
fn send_notification(title: &str, body: &str) {
    ShellCommand::new("osascript")
        .arg("-e")
        .arg(format!(
            "display notification \"{}\" with title \"{}\"",
            body.replace('"', "\\\""),
            title.replace('"', "\\\""),
        ))
        .status()
        .ok();
}

#[cfg(target_os = "linux")]
fn send_notification(title: &str, body: &str) {
    ShellCommand::new("notify-send")
        .arg(title)
        .arg(body)
        .status()
        .ok();
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn send_notification(title: &str, body: &str) {
    eprintln!("Desktop notifications are not supported on this platform.");
    eprintln!("{}: {}", title, body);
}
