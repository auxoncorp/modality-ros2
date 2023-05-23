use std::time::Duration;

use fxhash::FxHashMap;
use modality_api::{AttrVal, Nanoseconds, TimelineId, Uuid};
use modality_ingest_client::{
    dynamic::DynamicIngestClient, protocol::InternedAttrKey, IngestClient,
};

use crate::hooks::{CapturedMessageWithTime, MessageDirection};

pub struct MessageProcessor {
    client: modality_ingest_client::dynamic::DynamicIngestClient,
    attr_cache: FxHashMap<String, InternedAttrKey>,
    timeline_cache: FxHashMap<TimelineCacheKey, TimelineCacheValue>,
    currently_open_timeline: Option<TimelineId>,
    ordering: u128,
}

#[derive(Hash, Eq, PartialEq)]
pub struct TimelineCacheKey {
    node_namespace: &'static str,
    node_name: &'static str,
}

struct TimelineCacheValue {
    id: TimelineId,
}

impl MessageProcessor {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client =
            IngestClient::connect_with_standard_config(Duration::from_secs(20), None, None).await?;
        Ok(MessageProcessor {
            client: DynamicIngestClient::from(client),
            attr_cache: Default::default(),
            timeline_cache: Default::default(),
            ordering: 0,
            currently_open_timeline: None,
        })
    }

    pub async fn message_loop(
        mut self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<CapturedMessageWithTime<'static>>,
    ) {
        while let Some(captured_message) = rx.recv().await {
            // TODO top level errors, logging, ?
            let _ = self.handle_message(captured_message).await;
        }
    }

    async fn handle_message(
        &mut self,
        captured_message: CapturedMessageWithTime<'static>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.open_timeline(
            captured_message.msg.node_namespace,
            captured_message.msg.node_name,
            captured_message.msg.topic_name,
        )
        .await?;

        let mut event_name = captured_message.msg.topic_name.to_string();
        if event_name.starts_with('/') {
            event_name.remove(0);
        }

        let mut event_attrs = vec![(
            self.interned_attr_key("event.name").await?,
            AttrVal::String(event_name),
        )];

        match captured_message.msg.direction {
            MessageDirection::Send => {
                if let Some(t) = captured_message.publish_time.to_epoch_nanos() {
                    event_attrs.push((
                        self.interned_attr_key("event.timestamp").await?,
                        AttrVal::Timestamp(t),
                    ));
                }
                if let Some(gid) = captured_message.msg.publisher_graph_id {
                    event_attrs.push((
                        self.interned_attr_key("event.ros.publisher_gid").await?,
                        AttrVal::String(gid.to_string()),
                    ));
                }
            }
            MessageDirection::Receive => {
                if let Some(t) = captured_message.publish_time.to_epoch_nanos() {
                    event_attrs.push((
                        self.interned_attr_key("event.interaction.remote_event.timestamp")
                            .await?,
                        AttrVal::Timestamp(t),
                    ));
                }

                if let Some(gid) = captured_message.msg.publisher_graph_id {
                    event_attrs.push((
                        self.interned_attr_key("event.interaction.remote_event.ros.publisher_gid")
                            .await?,
                        AttrVal::String(gid.to_string()),
                    ));
                }

                let mut set_time = false;
                if let Some(ct) = captured_message.receive_time {
                    if let Some(t) = ct.to_epoch_nanos() {
                        // cyclonedds always sets this to 0.
                        if t.get_raw() != 0 {
                            event_attrs.push((
                                self.interned_attr_key("event.timestamp").await?,
                                AttrVal::Timestamp(t),
                            ));
                            set_time = true;
                        }
                    }
                }

                if !set_time {
                    if let Some(t) = now() {
                        event_attrs.push((
                            self.interned_attr_key("event.timestamp").await?,
                            AttrVal::Timestamp(t),
                        ))
                    }
                }
            }
        }

        for (k, v) in captured_message.msg.kvs {
            event_attrs.push((self.interned_attr_key(&format!("event.{k}")).await?, v));
        }

        self.client.event(self.ordering, event_attrs).await?;
        self.ordering += 1;

        Ok(())
    }

    // These strings are refs into a PublisherState, which stays put
    // on the heap after it's allocated, and never goes away.
    async fn open_timeline(
        &mut self,
        node_namespace: &'static str,
        node_name: &'static str,
        topic_name: &'static str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = TimelineCacheKey {
            node_namespace,
            node_name,
        };

        // If we have a cached id, just make this the active timeline and be done.
        if let Some(tl_cache_val) = self.timeline_cache.get(&key) {
            if self.currently_open_timeline != Some(tl_cache_val.id) {
                self.client.open_timeline(tl_cache_val.id).await?;
                self.currently_open_timeline = Some(tl_cache_val.id);
            }
            return Ok(());
        }

        // We haven't seen this timeline before, so send its metadata
        let timeline_id = TimelineId::from(Uuid::new_v4());
        self.client.open_timeline(timeline_id).await?;
        self.currently_open_timeline = Some(timeline_id);

        let tl_attrs = self
            .timeline_attrs(key.node_namespace, key.node_name, topic_name)
            .await?;
        self.client.timeline_metadata(tl_attrs).await?;

        self.timeline_cache
            .insert(key, TimelineCacheValue { id: timeline_id });
        Ok(())
    }

    async fn timeline_attrs(
        &mut self,
        node_namespace: &str,
        node_name: &str,
        topic_name: &str,
    ) -> Result<Vec<(InternedAttrKey, AttrVal)>, Box<dyn std::error::Error + Send + Sync>> {
        let mut timeline_name = format!("{node_namespace}{node_name}");
        if timeline_name.starts_with('/') {
            timeline_name.remove(0);
        }

        Ok(vec![
            (
                self.interned_attr_key("timeline.name").await?,
                AttrVal::String(timeline_name),
            ),
            (
                self.interned_attr_key("timeline.ros.topic").await?,
                AttrVal::String(topic_name.to_string()),
            ),
        ])
    }

    async fn interned_attr_key(
        &mut self,
        name: &str,
    ) -> Result<InternedAttrKey, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ik) = self.attr_cache.get(name) {
            return Ok(*ik);
        }

        let ik = self.client.declare_attr_key(name.to_string()).await?;
        self.attr_cache.insert(name.to_string(), ik);
        Ok(ik)
    }
}

fn now() -> Option<Nanoseconds> {
    let d = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d,
        Err(_e) => return None,
    };
    let fit_nanos: u64 = match d.as_nanos().try_into() {
        Ok(n) => n,
        Err(_) => return None,
    };
    Some(Nanoseconds::from(fit_nanos))
}