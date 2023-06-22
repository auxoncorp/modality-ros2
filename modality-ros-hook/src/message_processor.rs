use std::time::Duration;

use fxhash::{FxHashMap, FxHashSet};
use modality_api::{AttrVal, Nanoseconds, TimelineId};
use modality_ingest_client::{
    dynamic::DynamicIngestClient, protocol::InternedAttrKey, IngestClient,
};

use crate::hooks::{
    CapturedMessageWithTime, MessageDirection, PublisherGraphId, RmwEvent, RosEvent,
};

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

    pub async fn message_loop(mut self, mut rx: tokio::sync::mpsc::UnboundedReceiver<RosEvent>) {
        while let Some(event) = rx.recv().await {
            match event {
                RosEvent::Message(captured_message) => {
                    let _ = self.handle_message(captured_message).await;
                }
                RosEvent::Rmw(rmw_event) => {
                    let _ = self.handle_rmw_event(rmw_event).await;
                }
            }
        }
    }

    async fn handle_rmw_event(
        &mut self,
        event: RmwEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // TODO let timestamp = ();

        self.open_timeline(event.timeline_id()).await?;

        let mut kvs = vec![];

        if let Some(ts) = event.timestamp() {
            kvs.push((
                self.interned_attr_key("event.timestamp").await?,
                AttrVal::Timestamp(*ts),
            ));
        }

        match event {
            RmwEvent::CreateNode {
                node_timeline_id,
                node_namespace,
                node_name,
                timestamp: _,
            } => {
                self.send_timeline_metadata(&node_timeline_id, &node_namespace, &node_name)
                    .await?;
                kvs.push((
                    self.interned_attr_key("event.name").await?,
                    AttrVal::String("rmw.create_node".to_string()),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.node.namespace").await?,
                    AttrVal::String(node_namespace),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.node.name").await?,
                    AttrVal::String(node_name),
                ));
            }

            RmwEvent::DestroyNode {
                node_timeline_id: _,
                timestamp: _,
            } => {
                kvs.push((
                    self.interned_attr_key("event.name").await?,
                    AttrVal::String("rmw.destroy_node".to_string()),
                ));
            }

            RmwEvent::CreatePublisher {
                node_timeline_id: _,
                timestamp: _,
                gid,
                topic: topic_name,
                schema_namespace,
                schema_name,
                pub_count,
            } => {
                kvs.push((
                    self.interned_attr_key("event.name").await?,
                    AttrVal::String("rmw.create_publisher".to_string()),
                ));

                kvs.push((
                    self.interned_attr_key("event.internal.publisher.count").await?,
                    AttrVal::Integer(pub_count as i64),
                ));

                if let Some(gid) = gid {
                    kvs.push((
                        self.interned_attr_key("event.ros.publisher_gid").await?,
                        AttrVal::String(gid.to_string()),
                    ));
                }

                kvs.push((
                    self.interned_attr_key("event.ros.topic").await?,
                    AttrVal::String(topic_name),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.schema.namespace").await?,
                    AttrVal::String(schema_namespace),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.schema.name").await?,
                    AttrVal::String(schema_name),
                ));
            }

            RmwEvent::DestroyPublisher {
                node_timeline_id: _,
                timestamp: _,
                gid,
            } => {
                kvs.push((
                    self.interned_attr_key("event.name").await?,
                    AttrVal::String("rmw.destroy_publisher".to_string()),
                ));

                if let Some(gid) = gid {
                    kvs.push((
                        self.interned_attr_key("event.ros.publisher_gid").await?,
                        AttrVal::String(gid.to_string()),
                    ));
                }
            }

            RmwEvent::CreateSubscription {
                node_timeline_id: _,
                timestamp: _,
                topic: topic_name,
                schema_namespace,
                schema_name,
            } => {
                kvs.push((
                    self.interned_attr_key("event.name").await?,
                    AttrVal::String("rmw.create_subscription".to_string()),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.topic").await?,
                    AttrVal::String(topic_name),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.schema.namespace").await?,
                    AttrVal::String(schema_namespace),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.schema.name").await?,
                    AttrVal::String(schema_name),
                ));
            }

            RmwEvent::DestroySubscription {
                node_timeline_id: _,
                timestamp: _,
                topic: topic_name,
            } => {
                kvs.push((
                    self.interned_attr_key("event.name").await?,
                    AttrVal::String("rmw.destroy_subscription".to_string()),
                ));
                kvs.push((
                    self.interned_attr_key("event.ros.topic").await?,
                    AttrVal::String(topic_name),
                ));
            }
        }

        self.client.event(self.ordering, kvs).await?;
        self.ordering += 1;

        Ok(())
    }

    async fn handle_message(
        &mut self,
        captured_message: CapturedMessageWithTime,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.open_timeline_and_send_metadata(
            &captured_message.msg.node_timeline_id,
            &captured_message.msg.node_namespace,
            &captured_message.msg.node_name,
        )
        .await?;

        let mut event_attrs = vec![(
            self.interned_attr_key("event.ros.topic").await?,
            AttrVal::String(captured_message.msg.topic_name.clone()),
        )];

        let kvs = &captured_message.msg.kvs;
        if !kvs.iter().any(|(k, _)| k == "event.name" || k == "name") {
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

        for (mut k, v) in captured_message.msg.kvs {
            if !k.starts_with("event.") {
                k = format!("event.{k}");
            }
            event_attrs.push((self.interned_attr_key(&k).await?, v));
        }

        self.client.event(self.ordering, event_attrs).await?;
        self.ordering += 1;

        Ok(())
    }

    async fn open_timeline(
        &mut self,
        timeline_id: &TimelineId,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.currently_open_timeline.as_ref() != Some(timeline_id) {
            self.client.open_timeline(*timeline_id).await?;
            self.currently_open_timeline = Some(*timeline_id);
        }

        Ok(())
    }

    async fn send_timeline_metadata(
        &mut self,
        timeline_id: &TimelineId,
        node_namespace: &str,
        node_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let timeline_is_new = self.created_timelines.insert(*timeline_id);
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
                    AttrVal::String(node_name.to_string()),
                ),
                (
                    self.interned_attr_key("timeline.ros.node.namespace")
                        .await?,
                    AttrVal::String(node_namespace.to_string()),
                ),
                (
                    self.interned_attr_key("timeline.ros.node").await?,
                    AttrVal::String(format!("{}/{}", node_namespace, node_name)),
                ),
            ];

            self.client.timeline_metadata(tl_attrs).await?;
        }

        Ok(())
    }

    async fn open_timeline_and_send_metadata(
        &mut self,
        timeline_id: &TimelineId,
        node_namespace: &str,
        node_name: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.open_timeline(timeline_id).await?;
        self.send_timeline_metadata(timeline_id, node_namespace, node_name)
            .await
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
