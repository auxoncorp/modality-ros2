use crate::{
    interop::{self, rmw_get_gid_for_publisher, RmwGid, RmwMessageInfo},
    message_processor::MessageProcessor,
    FlatRosMessageSchema, RosMessageSchema,
};
use arc_swap::ArcSwap;
use archery::ArcK;
use lazy_static::lazy_static;
use modality_api::{AttrVal, Nanoseconds, TimelineId};

use std::{
    cell::RefCell,
    ffi::{c_char, c_int, c_long, c_void, CStr},
};

thread_local! {
    /// When we intercept a message from rmw_publish, it goes in
    /// here. Then the next time we see clock_gettime on the same
    /// thread (called by the DDS layer for the transport-level
    /// timestamp), we capture the time, add it to the message, and put
    /// it on SEND_CH for processing.
    pub static LAST_CAPTURED_MESSAGE: RefCell<Option<Vec<CapturedMessage>>> = RefCell::new(None);
}

lazy_static! {
    static ref NODES: ArcSwap<rpds::HashTrieMap<NodePtr, NodeState, ArcK>> = Default::default();
    static ref PUBLISHERS: ArcSwap<rpds::HashTrieMap<PublisherPtr, PublisherState, ArcK>> = Default::default();
    static ref SUBSCRIPTIONS: ArcSwap<rpds::HashTrieMap<SubscriptionPtr, SubscriptionState, ArcK>> = Default::default();

    static ref IGNORED_TOPICS: ArcSwap<rpds::HashTrieSet<String, ArcK>> = {
        let topics = rpds::HashTrieSet::default();
        // TODO make this configurable
        let topics = topics.insert("/parameter_events".to_string());
        let topics = topics.insert("/mavros/param/event".to_string());
        let topics = topics.insert("/mros/objective/_action/feedback".to_string());
        ArcSwap::new(std::sync::Arc::new(topics))
    };

    static ref SEND_CH: tokio::sync::mpsc::UnboundedSender<RosEvent> = {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel(); // TODO bounded?

        std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
                .unwrap();
            rt.block_on(async {
                let processor = MessageProcessor::new().await.expect("Initialize Modality message processor thread");
                processor.message_loop(rx).await;
            });
        });

        tx
    };
}

type NodePtr = usize;
struct NodeState {
    namespace: String,
    name: String,

    /// This will be used as this node's timeline id. It lets us
    /// distinguish this node from a similarly named one. These could
    /// live at the same time, or a node could be destroyed and
    /// re-created.
    timeline_id: TimelineId,
}

/// The unique identifier (gid) of a publisher.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct PublisherGraphId([u8; 16]);

impl std::fmt::Display for PublisherGraphId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("0x")?;
        for b in self.0.iter() {
            write!(f, "{b:02x}")?;
        }

        Ok(())
    }
}

pub type PublisherPtr = usize;
struct PublisherState {
    message_schema: FlatRosMessageSchema,
    topic_name: String,
    node_namespace: String,
    node_name: String,
    node_timeline_id: TimelineId,
    graph_id: Option<PublisherGraphId>,
}

#[derive(Debug)]
pub enum MessageDirection {
    Send {
        local_publisher_graph_id: Option<PublisherGraphId>,
    },
    Receive {
        remote_publisher_graph_id: Option<PublisherGraphId>,
    },
}

type SubscriptionPtr = usize;
struct SubscriptionState {
    message_schema: FlatRosMessageSchema,
    topic_name: String,
    node_namespace: String,
    node_name: String,
    node_timeline_id: TimelineId,
}

pub enum RosEvent {
    Message(CapturedMessageWithTime),
    Rmw(RmwEvent),
}

impl From<CapturedMessageWithTime> for RosEvent {
    fn from(value: CapturedMessageWithTime) -> Self {
        RosEvent::Message(value)
    }
}

impl From<RmwEvent> for RosEvent {
    fn from(value: RmwEvent) -> Self {
        RosEvent::Rmw(value)
    }
}

pub enum RmwEvent {
    CreateNode {
        node_namespace: String,
        node_name: String,
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
    },
    DestroyNode {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
    },
    CreatePublisher {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        gid: Option<PublisherGraphId>,
        topic: String,
        schema_namespace: String,
        schema_name: String,
        pub_count: usize
    },
    DestroyPublisher {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        gid: Option<PublisherGraphId>,
    },
    CreateSubscription {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        topic: String,
        schema_namespace: String,
        schema_name: String,
    },
    DestroySubscription {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        topic: String,
    },
}

fn now() -> Option<Nanoseconds> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| {
            let n: Option<u64> = d.as_nanos().try_into().ok();
            n.map(Nanoseconds::from)
        })
}

impl RmwEvent {
    pub fn create_node(
        node_namespace: String,
        node_name: String,
        node_timeline_id: TimelineId,
    ) -> RmwEvent {
        RmwEvent::CreateNode {
            node_namespace,
            node_name,
            node_timeline_id,
            timestamp: now(),
        }
    }

    pub fn destroy_node(node_timeline_id: TimelineId) -> RmwEvent {
        RmwEvent::DestroyNode {
            node_timeline_id,
            timestamp: now(),
        }
    }

    pub fn create_publisher(
        node_timeline_id: TimelineId,
        gid: Option<PublisherGraphId>,
        topic: String,
        schema_namespace: String,
        schema_name: String,
        pub_count: usize
    ) -> RmwEvent {
        RmwEvent::CreatePublisher {
            node_timeline_id,
            timestamp: now(),
            gid,
            topic,
            schema_namespace,
            schema_name,
            pub_count
        }
    }

    pub fn destroy_publisher(
        node_timeline_id: TimelineId,
        gid: Option<PublisherGraphId>,
    ) -> RmwEvent {
        RmwEvent::DestroyPublisher {
            node_timeline_id,
            timestamp: now(),
            gid,
        }
    }

    pub fn create_subscription(
        node_timeline_id: TimelineId,
        topic: String,
        schema_namespace: String,
        schema_name: String,
    ) -> RmwEvent {
        RmwEvent::CreateSubscription {
            node_timeline_id,
            timestamp: now(),
            topic,
            schema_namespace,
            schema_name,
        }
    }

    pub fn destroy_subscription(node_timeline_id: TimelineId, topic: String) -> RmwEvent {
        RmwEvent::DestroySubscription {
            node_timeline_id,
            timestamp: now(),
            topic,
        }
    }

    pub fn timeline_id(&self) -> &TimelineId {
        match self {
            RmwEvent::CreateNode {
                node_timeline_id, ..
            } => node_timeline_id,
            RmwEvent::DestroyNode {
                node_timeline_id, ..
            } => node_timeline_id,
            RmwEvent::CreatePublisher {
                node_timeline_id, ..
            } => node_timeline_id,
            RmwEvent::DestroyPublisher {
                node_timeline_id, ..
            } => node_timeline_id,
            RmwEvent::CreateSubscription {
                node_timeline_id, ..
            } => node_timeline_id,
            RmwEvent::DestroySubscription {
                node_timeline_id, ..
            } => node_timeline_id,
        }
    }

    pub fn timestamp(&self) -> &Option<Nanoseconds> {
        match self {
            RmwEvent::CreateNode { timestamp, .. } => timestamp,
            RmwEvent::DestroyNode { timestamp, .. } => timestamp,
            RmwEvent::CreatePublisher { timestamp, .. } => timestamp,
            RmwEvent::DestroyPublisher { timestamp, .. } => timestamp,
            RmwEvent::CreateSubscription { timestamp, .. } => timestamp,
            RmwEvent::DestroySubscription { timestamp, .. } => timestamp,
        }
    }
}

#[derive(Debug)]
pub struct CapturedMessage {
    pub kvs: Vec<(String, AttrVal)>,
    pub topic_name: String,
    pub node_namespace: String,
    pub node_name: String,
    pub node_timeline_id: TimelineId,
    pub direction: MessageDirection,
}

#[derive(Debug)]
pub struct CapturedMessageWithTime {
    pub msg: CapturedMessage,
    pub publish_time: CapturedTime,
    pub receive_time: Option<CapturedTime>,
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CapturedTime {
    Compound { sec: i64, nsec: i64 },
    SignedEpochNanos(i64),
}

impl CapturedTime {
    pub fn to_epoch_nanos(self) -> Option<Nanoseconds> {
        match self {
            CapturedTime::Compound { sec, nsec } => {
                if sec < 0 || nsec < 0 {
                    return None;
                }
                let sec = sec as u64;
                let nsec = nsec as u64;
                Some(Nanoseconds::from(
                    sec.checked_mul(1_000_000_000_u64)? + nsec,
                ))
            }
            CapturedTime::SignedEpochNanos(ns) => {
                if ns < 0 {
                    None
                } else {
                    Some(Nanoseconds::from(ns as u64))
                }
            }
        }
    }
}

redhook::hook! {
    unsafe fn rmw_create_node(context: *const c_void, name: *const c_char, namespace: *const c_char) -> NodePtr
        => modality_hook_rmw_create_node
    {
        let node_name = String::from_utf8_lossy(CStr::from_ptr(name).to_bytes()).to_string();
        let node_namespace = String::from_utf8_lossy(CStr::from_ptr(namespace).to_bytes()).to_string();

        let node_address = redhook::real!(rmw_create_node)(context, name, namespace);
        let node_timeline_id = TimelineId::allocate();

        NODES.rcu(|nodes| {
            nodes.insert(node_address, NodeState {
                name: node_name.clone(), namespace: node_namespace.clone(), timeline_id: node_timeline_id })
        });

        let _ = SEND_CH.send(RmwEvent::create_node(node_namespace, node_name, node_timeline_id).into());

        node_address
    }
}

redhook::hook! {
    unsafe fn rmw_destroy_node(node_ptr: NodePtr) -> c_int
        => modality_hook_rmw_destroy_node
    {

        if let Some(node_state) = NODES.load().get(&node_ptr) {
            let _ = SEND_CH.send(RmwEvent::destroy_node(node_state.timeline_id).into());
        }

        let ret = redhook::real!(rmw_destroy_node)(node_ptr);
        NODES.rcu(|nodes| nodes.remove(&node_ptr));

        ret
    }
}

redhook::hook! {
    unsafe fn rmw_create_publisher(node: NodePtr, type_support: *const interop::RosIdlMessageTypeSupportT,
                                   topic_name: *const c_char, qos_policies: *const c_void,
                                   publisher_options: *const c_void) -> PublisherPtr
        => modality_hook_rmw_create_publisher
    {
        let topic_name_str = String::from_utf8_lossy(CStr::from_ptr(topic_name).to_bytes()).trim().to_string();
        if IGNORED_TOPICS.load().contains(&topic_name_str) {
            // shortcut return for ignored topics
            return redhook::real!(rmw_create_publisher)
                (node, type_support, topic_name, qos_policies, publisher_options);
        }


        //println!("topic: {}", topic_name_str);
        let mut maybe_schema = RosMessageSchema::from_c(type_support);

        if let Some(node_state) = NODES.load().get(&node) {
            let publisher_address = redhook::real!(rmw_create_publisher)
                (node, type_support, topic_name, qos_policies, publisher_options);

            let mut graph_id = None;
            let mut c_gid = RmwGid::default();
            if rmw_get_gid_for_publisher(publisher_address, &mut c_gid) == 0 {
                graph_id = Some(PublisherGraphId(c_gid.data));
            }

            let mut pub_count = 0;
            if let Some(message_schema) = maybe_schema.take() {
                PUBLISHERS.rcu(|pubs| {
                    pub_count = pubs.size() + 1;
                    pubs.insert(
                        publisher_address,
                        PublisherState {
                            message_schema: message_schema.clone().flatten(&vec![]),
                            topic_name: topic_name_str.clone(),
                            node_namespace: node_state.namespace.clone(),
                            node_name: node_state.name.clone(),
                            node_timeline_id: node_state.timeline_id,
                            graph_id
                        }
                    )
                });

                let _ = SEND_CH.send(
                    RmwEvent::create_publisher(node_state.timeline_id, graph_id, topic_name_str,
                                               message_schema.namespace, message_schema.name, pub_count).into()
                );
            }
            publisher_address
        } else {
            // If someone tries to create a publisher against an
            // uninitialized node, I guess? This is probably an error,
            // and really shouldn't be happening.
            redhook::real!(rmw_create_publisher)
                (node, type_support, topic_name, qos_policies, publisher_options)
        }
    }
}

redhook::hook! {
    unsafe fn rmw_destroy_publisher(node_ptr: NodePtr, pub_ptr: PublisherPtr) -> c_int
        => modality_hook_rmw_destroy_publisher
    {

        if let Some(pub_state) = PUBLISHERS.load().get(&pub_ptr) {
            let _ = SEND_CH.send(RmwEvent::destroy_publisher(pub_state.node_timeline_id, pub_state.graph_id).into());
        }

        let ret = redhook::real!(rmw_destroy_publisher)(node_ptr, pub_ptr);
        PUBLISHERS.rcu(|pubs| pubs.remove(&pub_ptr));
        ret
    }
}

redhook::hook! {
    unsafe fn rmw_publish(publisher_address: PublisherPtr, message: *const c_void, allocation: *const c_void) -> c_int
        => modality_hook_rmw_publish
    {
        if let Some(pub_state) = PUBLISHERS.load().get(&publisher_address) {
            let message_size = pub_state.message_schema.size;
            let src_message_bytes: &[u8] = std::slice::from_raw_parts(message as *const u8, message_size);

            let mut captured_messages = vec![];
            for kvs in pub_state.message_schema.interpret_message(src_message_bytes) {
                let topic_name = pub_state.topic_name.clone();
                let node_namespace = pub_state.node_namespace.clone();
                let node_name = pub_state.node_name.clone();
                let node_timeline_id = pub_state.node_timeline_id;

                let direction = MessageDirection::Send { local_publisher_graph_id: pub_state.graph_id };
                let captured_message = CapturedMessage { kvs, topic_name, node_namespace, node_name, direction, node_timeline_id };
                captured_messages.push(captured_message);
            }

            let _called_after_dest = LAST_CAPTURED_MESSAGE.try_with(|lcm| {
                *lcm.borrow_mut() = Some(captured_messages);
            }).is_err();
        }

        redhook::real!(rmw_publish)(publisher_address, message, allocation)
    }
}

#[repr(C)]
pub struct TimeSpec {
    tv_sec: c_long, // actually time_t
    tv_nsec: c_long,
}

redhook::hook! {
    unsafe fn clock_gettime(clock_id: usize, timespec: *const TimeSpec) -> c_int
        => modality_hook_clock_gettime
    {
        // Don't print anything in here! You'll break pieces of ROS
        // that expect certain things on stdout AND stderr.

        let res = redhook::real!(clock_gettime)(clock_id, timespec);

        // return value 0 means success
        //
        // clock id 0 is the regular wall clock. fastdds does some
        // other calls for a different id before this one, which is
        // attached to the message.
        if res == 0 && clock_id == 0 {
            let _called_after_dest = LAST_CAPTURED_MESSAGE.try_with(|b| {
                if let Some(msgs) = b.borrow_mut().take() {
                    for msg in msgs.into_iter() {
                        let msg_with_time = CapturedMessageWithTime {
                            msg,
                            publish_time: CapturedTime::Compound { sec: (*timespec).tv_sec, nsec: (*timespec).tv_nsec },
                            receive_time: None
                        };
                        // intentionally ignore errors here. This will
                        // fail if the rx thread is dead, but in that case
                        // a message has already been printed, and this
                        // would just be belaboring the point, repeatedly.
                        let _ = SEND_CH.send(msg_with_time.into());
                    }
                }
            }).is_err();
        }

        res
    }
}

redhook::hook! {
    unsafe fn rmw_create_subscription(node: NodePtr, type_support: *const interop::RosIdlMessageTypeSupportT,
                                      topic_name: *const c_char, qos_policies: *const c_void,
                                      subscription_options: *const c_void) -> SubscriptionPtr
        => modality_hook_rmw_create_subscription

    {
        let topic_name_str = String::from_utf8_lossy(CStr::from_ptr(topic_name).to_bytes()).trim().to_string();
        if IGNORED_TOPICS.load().contains(&topic_name_str) {
            // shortcut return for ignored topics

            return redhook::real!(rmw_create_subscription)
                (node, type_support, topic_name, qos_policies, subscription_options);
        }

        if let Some(node_state) = NODES.load().get(&node) {
            let subscription_address = redhook::real!(rmw_create_subscription)
                (node, type_support, topic_name, qos_policies, subscription_options);

            let mut maybe_schema = RosMessageSchema::from_c(type_support);
            if let Some(message_schema) = maybe_schema.take() {
                SUBSCRIPTIONS.rcu(|subs| {
                    subs.insert(
                        subscription_address,
                        SubscriptionState {
                            message_schema: message_schema.clone().flatten(&vec![]),
                            topic_name: topic_name_str.clone(),
                            node_namespace: node_state.namespace.clone(),
                            node_name: node_state.name.clone(),
                            node_timeline_id: node_state.timeline_id,
                        }
                    )
                });

                let _ = SEND_CH.send(
                    RmwEvent::create_subscription(
                        node_state.timeline_id, topic_name_str, message_schema.namespace, message_schema.name
                    ).into()
                );
            }
            subscription_address
        } else {
            redhook::real!(rmw_create_subscription)
                (node, type_support, topic_name, qos_policies, subscription_options)
        }
    }
}

redhook::hook! {
    unsafe fn rmw_destroy_subscription(node_ptr: NodePtr, sub_ptr: SubscriptionPtr) -> c_int
        => modality_hook_rmw_destroy_subscription
    {
        if let Some(sub_state) = SUBSCRIPTIONS.load().get(&sub_ptr) {
            let _ = SEND_CH.send(RmwEvent::destroy_subscription(sub_state.node_timeline_id, sub_state.topic_name.clone()).into());
        }

        let ret = redhook::real!(rmw_destroy_subscription)(node_ptr, sub_ptr);
        SUBSCRIPTIONS.rcu(|subs| subs.remove(&sub_ptr));
        ret
    }
}

redhook::hook! {
    unsafe fn rmw_take_with_info(subscription_address: PublisherPtr,
                                 message: *mut c_void, taken: *mut bool, message_info: *mut RmwMessageInfo,
                                 allocation: *mut c_void) -> c_int
        => modality_hook_rmw_take_with_info
    {

        let res = redhook::real!(rmw_take_with_info)(subscription_address, message, taken, message_info, allocation);
        // 0 is success
        if res == 0 {
            if let Some(sub_state) = SUBSCRIPTIONS.load().get(&subscription_address) {
                let message_size = sub_state.message_schema.size;
                let src_message_bytes: &[u8] = std::slice::from_raw_parts(message as *const u8, message_size);
                for kvs in sub_state.message_schema.interpret_message(src_message_bytes) {
                    let topic_name = sub_state.topic_name.clone();
                    let node_namespace = sub_state.node_namespace.clone();
                    let node_name = sub_state.node_name.clone();
                    let node_timeline_id = sub_state.node_timeline_id;

                    let remote_publisher_graph_id = Some(PublisherGraphId((*message_info).publisher_gid.data));
                    let direction = MessageDirection::Receive { remote_publisher_graph_id };

                    let msg = CapturedMessageWithTime {
                        msg: CapturedMessage { kvs, topic_name, node_namespace, node_name, direction, node_timeline_id },
                        publish_time: CapturedTime::SignedEpochNanos((*message_info).source_timestamp),
                        receive_time: Some(CapturedTime::SignedEpochNanos((*message_info).received_timestamp)),
                    };

                    // intentionally ignore errors here. This will fail if
                    // the rx thread is dead, but in that case a message
                    // has already been printed, and this would just be
                    // belaboring the point, repeatedly.
                    let _ = SEND_CH.send(msg.into());
                }
            }
        }

        res
    }
}

// TODO take sequence

// RMW_PUBLIC
// RMW_WARN_UNUSED
// rmw_ret_t
// rmw_take_sequence(
//   const rmw_subscription_t * subscription,
//   size_t count,
//   rmw_message_sequence_t * message_sequence,
//   rmw_message_info_sequence_t * message_info_sequence,
//   size_t * taken,
//   rmw_subscription_allocation_t * allocation);
