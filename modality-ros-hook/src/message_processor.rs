use std::time::Duration;

use fxhash::{FxHashMap, FxHashSet};
use modality_api::{AttrVal, Nanoseconds, TimelineId};
use modality_ingest_client::{
    dynamic::DynamicIngestClient, protocol::InternedAttrKey, IngestClient,
};

use crate::hooks::{CapturedMessageWithTime, MessageDirection, PublisherGraphId};

pub struct MessageProcessor {
    client: modality_ingest_client::dynamic::DynamicIngestClient,
    attr_key_cache: FxHashMap<String, InternedAttrKey>,
    created_timelines: FxHashSet<TimelineId>,
    sent_timeline_publisher_metadata: FxHashSet<(TimelineId, PublisherGraphId)>,
    currently_open_timeline: Option<TimelineId>,
    ordering: u128,
}

impl MessageProcessor {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client =
            IngestClient::connect_with_standard_config(Duration::from_secs(20), None, None).await?;
        Ok(MessageProcessor {
            client: DynamicIngestClient::from(client),
            attr_key_cache: Default::default(),
            created_timelines: Default::default(),
            sent_timeline_publisher_metadata: Default::default(),
            ordering: 0,
            currently_open_timeline: None,
        })
    }

    pub async fn message_loop(
        mut self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<CapturedMessageWithTime>,
    ) {
        while let Some(captured_message) = rx.recv().await {
            // TODO top level errors, logging, ?
            let _ = self.handle_message(captured_message).await;
        }
    }

    async fn handle_message(
        &mut self,
        captured_message: CapturedMessageWithTime,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.open_timeline(
            captured_message.msg.node_namespace,
            captured_message.msg.node_name,
            captured_message.msg.node_timeline_id,
        )
        .await?;

        let mut event_attrs = vec![(
            self.interned_attr_key("event.ros.topic").await?,
            AttrVal::String(captured_message.msg.topic_name.clone()),
        )];

        let kvs = &captured_message.msg.kvs;
        if !kvs.iter().any(|(k, _)| k == "name") {
            // If there's no incoming event name, use a normalized version of the topic name
            let mut event_name = captured_message.msg.topic_name.to_string();
            if event_name.starts_with('/') {
                event_name.remove(0);
            }
            event_attrs.push((
                self.interned_attr_key("event.name").await?,
                AttrVal::String(event_name),
            ));
        }

        // http://wiki.ros.org/Names claims that names can't have '.' in them
        let mut normalized_topic_name = captured_message.msg.topic_name.clone();
        if normalized_topic_name.starts_with('/') {
            normalized_topic_name.remove(0);
        }
        let normalized_topic_name = normalized_topic_name.replace('/', ".");

        match captured_message.msg.direction {
            MessageDirection::Send {
                local_publisher_graph_id,
            } => {
                event_attrs.push((
                    self.interned_attr_key("event.ros.publish").await?,
                    AttrVal::Bool(true),
                ));

                if let Some(local_gid) = local_publisher_graph_id {
                    if self
                        .sent_timeline_publisher_metadata
                        .insert((captured_message.msg.node_timeline_id, local_gid))
                    {
                        let attrs = vec![(
                            self.interned_attr_key(&format!(
                                "timeline.ros.publisher_gid.{normalized_topic_name}"
                            ))
                            .await?,
                            AttrVal::String(local_gid.to_string()),
                        )];

                        self.client.timeline_metadata(attrs).await?;
                    }
                }

                if let Some(t) = captured_message.publish_time.to_epoch_nanos() {
                    event_attrs.push((
                        self.interned_attr_key("event.timestamp").await?,
                        AttrVal::Timestamp(t),
                    ));
                }

                match captured_message.publish_time {
                    crate::hooks::CapturedTime::Compound { sec, nsec } => {
                        event_attrs.push((
                            self.interned_attr_key("event.dds_time.sec").await?,
                            AttrVal::Integer(sec),
                        ));
                        event_attrs.push((
                            self.interned_attr_key("event.dds_time.nanosec").await?,
                            AttrVal::Integer(nsec),
                        ));
                    }
                    crate::hooks::CapturedTime::SignedEpochNanos(_) => todo!(),
                }
            }
            MessageDirection::Receive {
                remote_publisher_graph_id,
            } => {
                event_attrs.push((
                    self.interned_attr_key("event.ros.receive").await?,
                    AttrVal::Bool(true),
                ));

                if let Some(t) = captured_message.publish_time.to_epoch_nanos() {
                    event_attrs.push((
                        self.interned_attr_key("event.interaction.remote_event.timestamp")
                            .await?,
                        AttrVal::Timestamp(t),
                    ));
                }

                if let Some(gid) = remote_publisher_graph_id {
                    event_attrs.push((
                        self.interned_attr_key(
                            &format!("event.interaction.remote_timeline.ros.publisher_gid.{normalized_topic_name}")
                        )
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
        node_namespace: String,
        node_name: String,
        timeline_id: TimelineId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client.open_timeline(timeline_id).await?;

        let timeline_is_new = self.created_timelines.insert(timeline_id);
        if timeline_is_new {
            let mut timeline_name = format!("{node_namespace}/{node_name}");
            while timeline_name.starts_with('/') {
                timeline_name.remove(0);
            }

            let tl_attrs = vec![
                (
                    self.interned_attr_key("timeline.name").await?,
                    AttrVal::String(timeline_name),
                ),
                (
                    self.interned_attr_key("timeline.ros.node.name").await?,
                    AttrVal::String(node_name.clone()),
                ),
                (
                    self.interned_attr_key("timeline.ros.node.namespace")
                        .await?,
                    AttrVal::String(node_namespace.clone()),
                ),
                (
                    self.interned_attr_key("timeline.ros.node").await?,
                    AttrVal::String(format!("{}/{}", node_namespace, node_name)),
                ),
            ];

            self.client.timeline_metadata(tl_attrs).await?;
        }

        self.currently_open_timeline = Some(timeline_id);

        Ok(())
    }

    async fn interned_attr_key(
        &mut self,
        name: &str,
    ) -> Result<InternedAttrKey, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(ik) = self.attr_key_cache.get(name) {
            return Ok(*ik);
        }

        let ik = self.client.declare_attr_key(name.to_string()).await?;
        self.attr_key_cache.insert(name.to_string(), ik);
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
