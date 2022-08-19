use glob::glob;
use uuid::Uuid;
use std::{path::PathBuf, collections::BTreeMap};
use anyhow::{Result, Context, bail};
use chrono::{NaiveDateTime, Local, Duration, DateTime, Date};
use clap::{Parser, Subcommand};
use tsk_rs::{settings::Settings, task::Task};

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ReportError {
    #[error("End date cant be in the past")]
    EndDateInThePast

}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum SummaryInterval {
    Daily,
}

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Sets a config file
    #[clap(short, long, value_parser, value_name = "FILE", default_value = "tsk.toml")]
    config: PathBuf,

    #[clap(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Reports task summaries
    #[clap(trailing_var_arg = true)]
    Summary {
        /// Existing task id or a part of one
        #[clap(value_parser)]
        id: Option<String>,
        /// Select summary interval
        #[clap(long,value_enum)]
        interval: SummaryInterval,
        /// Start time of the summary listing
        #[clap(long,value_parser)]
        start_date: NaiveDateTime,
        /// End time of the summary listing
        #[clap(long,value_parser)]
        end_date: Option<NaiveDateTime>,
        /// Include also completed tasks
        #[clap(short, long, value_parser)]
        include_done: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let settings = Settings::new(cli.config.to_str().unwrap())
        .with_context(|| {"while loading settings"})?;

    match &cli.command {
        Some(Commands::Summary { id, interval, start_date, end_date, include_done }) => {
            let duration = match end_date {
                Some(end_date) => *end_date - *start_date,
                None => {Local::now().naive_local() - *start_date},
            };
            if duration.num_seconds() <= 0 {
                bail!(ReportError::EndDateInThePast);
            }
            summary_report(id, interval, start_date, &duration, include_done, &settings)
        },
        None => {todo!("default action not implemented")},
    }
}

fn summary_report(id: &Option<String>, interval: &SummaryInterval, start_date: &NaiveDateTime, duration: &Duration, include_done: &bool, settings: &Settings) -> Result<()> {
    let mut task_pathbuf: PathBuf = settings.task_db_pathbuf().with_context(|| {"invalid data directory path configured"})?;
    if id.is_some() {
        task_pathbuf = task_pathbuf.join(format!("*{}*.yaml", id.as_ref().unwrap()));
    } else {
        task_pathbuf = task_pathbuf.join("*.yaml");
    }

    let mut found_tasks: Vec<Task> = vec![];
    for task_filename in glob(task_pathbuf.to_str().unwrap()).with_context(|| {"while traversing task data directory files"})? {
        // if the filename is u-u-i-d.3.yaml for example it is a backup file and should be disregarded
        if task_filename.as_ref().unwrap().file_name().unwrap().to_string_lossy().split('.').collect::<Vec<_>>()[1] != "yaml" {
            continue;
        }
        let task = Task::load_yaml_file_from(&task_filename?).with_context(|| {"while loading task from yaml file"})?;
        if !task.done || *include_done {
            found_tasks.push(task);
        }   
    }

    match *interval {
        SummaryInterval::Daily => daily_summary(found_tasks, start_date, duration),
    }
}

fn daily_summary(tasks: Vec<Task>, start_date: &NaiveDateTime, duration: &Duration) -> Result<()> {
    let mut summary: BTreeMap<Date<Local>, BTreeMap<Uuid, Duration>> = BTreeMap::new();

    let default_duration = Duration::seconds(0);

    for task in tasks {
        for day in 0..duration.num_days() {
            let test_date = start_date.and_local_timezone(Local).unwrap() + Duration::days(day);

            let mut inner: BTreeMap<Uuid, Duration> = BTreeMap::new();

            let create_date = task.create_date()?;
            if create_date.date() == test_date.date() {
                // task was created today
                inner.insert(task.id, default_duration); // todo: set default duration from config?
            }

            if let Some(done_date) = task.done_date()? {
                if done_date.date() == test_date.date() {
                    let total_duration = *inner.get(&task.id).unwrap_or(&default_duration);
                    let new_duration = total_duration + default_duration;
                    inner.insert(task.id, new_duration); // todo: set default duration from config?
                }
            }

            if let Some(timetracks) = task.timetracker.clone() {
                for timetrack in timetracks {
                    if let Some(duration) = timetrack.duration() {
                        // timetrack is running so it has a duration
                        let tr_end_time = timetrack.end_time.unwrap();
                        if timetrack.start_time.date() == test_date.date() && tr_end_time.date() == test_date.date() {
                            // timetrack was started today and it ended today
                            let total_duration = *inner.get(&task.id).unwrap_or(&default_duration);
                            let new_duration = total_duration + duration;
                            inner.insert(task.id, new_duration);
                            continue;
                        }
                        
                        if timetrack.start_time.date() == test_date.date() || tr_end_time.date() == test_date.date() {
                            // timetrack was started or stopped today
                            let end_of_day = test_date.date() + Duration::hours(24);
                            continue;
                        }
                        
                        if timetrack.start_time.date() != test_date.date() && tr_end_time.date() != test_date.date() {
                            // timetrack was not running today or it ran for the entire day
                            continue;
                        }
                    }
                }
            }
            summary.insert(test_date.date(), inner);
        }
    }
    println!("{:?}", summary);

    Ok(())
}


// eof