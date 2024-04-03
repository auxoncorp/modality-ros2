use fxhash::FxHashSet;
use lazy_static::lazy_static;
use std::time::Duration;

lazy_static! {
    pub static ref CONFIG: ModalityRosHookConfig = ModalityRosHookConfig::from_env();
}

pub struct ModalityRosHookConfig {
    pub connect_timeout: Duration,
    pub ignored_topics: FxHashSet<String>,
    pub max_array_len: usize,
}

impl ModalityRosHookConfig {
    pub fn from_env() -> Self {
        let mut connect_timeout = Duration::from_secs(20);
        if let Ok(s) = std::env::var("MODALITY_ROS_CONNECT_TIMEOUT") {
            if let Ok(t) = s.parse::<u64>() {
                connect_timeout = Duration::from_secs(t);
            }
        }

        let mut ignored_topics = FxHashSet::default();
        if let Ok(s) = std::env::var("MODALITY_ROS_IGNORED_TOPICS") {
            for topic in s.split(',') {
                ignored_topics.insert(topic.trim().to_string());
            }
        } else {
            ignored_topics.insert("/parameter_events".to_string());
        }

        let mut max_array_len = 12;
        if let Ok(s) = std::env::var("MODALITY_ROS_MAX_ARRAY_LEN") {
            if let Ok(l) = s.parse::<usize>() {
                max_array_len = l;
            }
        }

        ModalityRosHookConfig {
            max_array_len,
            ignored_topics,
            connect_timeout,
        }
    }
}
