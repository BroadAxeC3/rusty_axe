use aws_config::meta::region::RegionProviderChain;
use aws_config::imds::client::{Client as IMDS_Client};
use aws_sdk_cloudwatchlogs::model::InputLogEvent;
use aws_sdk_cloudwatchlogs::{Client as CWL_Client, Error};

use clap::Parser;
use chrono;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek};
use std::time::{SystemTime, UNIX_EPOCH};

/// Quickly shove a file into CloudWatch Logs
///
/// Some times you need to keep a little bit of log data for debugging purposes
/// or perhaps you need to document why an EC2 instance keeps crashing.  This is
/// where I come in.  Call me right before the instance goes down and I'll do my
/// best to jam as much (or as little) information into CloudWatch Logs as I can.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path of the file to process
    #[clap(short, long)]
    filename: String,

    /// CloudWatchLogs group to write messages to
    #[clap(short, long)]
    group: String,

    /// Process the first lines of the file
    #[clap(short, long, default_value_t = 0)]
    head: usize,

    /// Process the last lines of the file
    #[clap(short, long, default_value_t = 0)]
    tail: usize,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();

    let events = get_events(args.filename, args.head, args.tail).await?;
    send_logs(args.group, events).await?;

    Ok(())
}


async fn get_events(path: String, head: usize, tail: usize) -> Result<Vec<InputLogEvent>, Error> {
    println!("Reading {:?}...", path);

    let timestamp: i64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap();

    // Open file and get it ready to process...
    let mut file = File::open(path).unwrap();
    let reader = BufReader::new(&file);
    let count = reader.lines().count();
    file.rewind().unwrap();
    let reader = BufReader::new(&file);

    let mut events = Vec::new();

    // Create a set of log events from the file contents
    for (index, line) in reader.lines().enumerate() {
        let mut line = line.unwrap();
        let mut event = InputLogEvent::builder().build();

        // CloudWatch Logs doesn't like blank lines
        if line.len() == 0 {
            line = String::from(" ");
        }

        // Log all the lines
        if head == 0 && tail == 0 {
            event = InputLogEvent::builder()
                .timestamp(timestamp)
                .message(&line)
                .build();
        }
        // Log the first line(s)
        if head != 0 && index < head {
            event = InputLogEvent::builder()
                .timestamp(timestamp)
                .message(&line)
                .build();
        }
        // Log the last line(s)
        if (tail >= count) || (tail != 0 && index > count - (tail + 1)) {
            event = InputLogEvent::builder()
                .timestamp(timestamp)
                .message(&line)
                .build();
        }

        match event.message {
            Some(_) => events.push(event),
            None => (),
        }
    }

    Ok(events)
}

async fn send_logs(group: String, events: Vec<InputLogEvent>) -> Result<(), Error> {
    // Prepare AWS configs...
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let cwlogs = CWL_Client::new(&config);
    let imds = IMDS_Client::builder().build().await.expect("valid client");

    // let timestamp: i64 = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros().try_into().unwrap();
    let timestamp = chrono::offset::Utc::now().format("%F_%H-%M-%S-%f").to_string();
    let mut instance_id = String::from("i-00000000000000000");
    match imds
        .get("/latest/meta-data/instance-id")
        .await
        {
            Ok(result) => instance_id = result,
            Err(e) => eprintln!("Couldn't retrieve instance_id: {}", e),
        }
    let log_stream_name = format!("{}-{}", instance_id, timestamp);

    // In order to post to a log stream you have to have a sequence number (except
    // for the fisrt time).  So, since we don't memoize the sequence id from previous runs,
    // we have to create a new log stream every time we process a file.
    match cwlogs
        .create_log_stream()
        .log_group_name(&group)
        .log_stream_name(&log_stream_name)
        .send()
        .await
    {
        Ok(_) => println!("Created new log stream: {}", &log_stream_name),
        Err(aws_sdk_cloudwatchlogs::types::SdkError::ServiceError { err, .. }) if err.is_resource_already_exists_exception() => {
            println!("Log stream already exists");
        }
        Err(e) => {
            panic!("Error creating log stream: {}", e)
        }
    }


    let resp = cwlogs
        .put_log_events()
        .log_group_name(&group)
        .log_stream_name(&log_stream_name)
        .set_log_events(Some(events))
        .send()
        .await?;

    let mut next_sequence = String::from("");
    match resp.next_sequence_token {
        Some(ref token) => next_sequence = token.to_string(),
        None => println!("No more logs to send"),
    }

    match resp.rejected_log_events_info {
        Some(e) => eprintln!("Some logs were rejected: {:#?}", e),
        None => println!(""),
    }

    println!("RESP: {:?}", next_sequence);

    Ok(())
}
