use auxon_sdk::api::Uuid;
use fxhash::FxHashSet;
use lazy_static::lazy_static;
use std::{env::VarError, time::Duration};

lazy_static! {
    pub static ref CONFIG: ModalityRosHookConfig = ModalityRosHookConfig::from_env();
}

pub struct ModalityRosHookConfig {
    /// Connection timeout for the modality backend
    /// connection. Configured with the MODALITY_ROS_CONNECT_TIMEOUT
    /// environment var (in seconds). Defaults to 20.
    pub connect_timeout: Duration,

    /// A list of full ros topic names to be ignored. Configured with
    /// the MODALITY_ROS_IGNORED_TOPICS env var, as a comma-separated
    /// list. Defaults to '/parameter_events'.
    pub ignored_topics: FxHashSet<String>,

    /// The longest array value to ingest, in a message
    /// payload. Arrays longer than this will be ignored. Configured
    /// with the MODALITY_ROS_MAX_ARRAY_LEN env var. Defaults to 12.
    pub max_array_len: usize,

    /// The run id to value to use in timeline metadata
    /// 'timeline.run_id'. Configured with the MODALITY_RUN_ID env
    /// var. Defaults to a randomly generated uuid.
    pub run_id: String,

    /// How many messages to buffer internally, before sending to the
    /// backend.  Configured with the MODALITY_ROS_BUFFER_SIZE env
    /// var. Defaults to 1024.
    pub buffer_size: usize,
}

impl ModalityRosHookConfig {
    pub fn from_env() -> Self {
        let mut connect_timeout = Duration::from_secs(20);
        if let Some(t) = u64_env_var("MODALITY_ROS_CONNECT_TIMEOUT") {
            connect_timeout = Duration::from_secs(t);
        }

        let mut ignored_topics = FxHashSet::default();
        if let Some(s) = string_env_var("MODALITY_ROS_IGNORED_TOPICS") {
            for topic in s.split(',') {
                ignored_topics.insert(topic.trim().to_string());
            }
        } else {
            ignored_topics.insert("/parameter_events".to_string());
        }

        let mut max_array_len = 12;
        if let Some(l) = usize_env_var("MODALITY_ROS_MAX_ARRAY_LEN") {
            max_array_len = l;
        }

        let mut buffer_size = 1024;
        if let Some(s) = usize_env_var("MODALITY_ROS_BUFFER_SIZE") {
            buffer_size = s;
        }

        let mut run_id = Uuid::new_v4().to_string();
        if let Some(s) = string_env_var("MODALITY_RUN_ID") {
            run_id = s;
        }

        ModalityRosHookConfig {
            max_array_len,
            ignored_topics,
            connect_timeout,
            run_id,
            buffer_size,
        }
    }
}

fn u64_env_var(name: &str) -> Option<u64> {
    let s = string_env_var(name)?;
    match s.trim().parse::<u64>() {
        Ok(n) => Some(n),
        Err(e) => {
            eprintln!("MODALITY: Environment variable '{name}' is not valid ({e:?})");
            None
        }
    }
}

fn usize_env_var(name: &str) -> Option<usize> {
    let s = string_env_var(name)?;
    match s.trim().parse::<usize>() {
        Ok(n) => Some(n),
        Err(e) => {
            eprintln!("MODALITY: Environment variable '{name}' is not valid ({e:?})");
            None
        }
    }
}

fn string_env_var(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(s) => Some(s),
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(_)) => {
            eprintln!("MODALITY: Environment variable '{name}' is not valid (invalid unicode)");
            None
        }
    }
}
