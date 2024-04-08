use crate::{
    config::CONFIG, message_processor::MessageProcessor, ros, FlatRosMessageSchema,
    RosMessageSchema, RosServiceSchema,
};
use arc_swap::ArcSwap;
use archery::ArcK;
use auxon_sdk::api::{AttrVal, Nanoseconds, TimelineId};
use lazy_static::lazy_static;

use std::{
    cell::RefCell,
    ffi::{c_char, c_long, c_void, CStr},
};

thread_local! {
    /// When we intercept a message from rmw_publish, it goes in here. Then the next time we see clock_gettime on the same
    /// thread (called by the DDS layer for the transport-level timestamp), we capture the time, add it to the message, and put
    /// it on SEND_CH for processing.
    pub static LAST_CAPTURED_MESSAGE: RefCell<Option<Vec<CapturedMessage>>> = const { RefCell::new(None) }
}

lazy_static! {
    static ref NODES: ArcSwap<rpds::HashTrieMap<NodePtr, NodeState, ArcK>> = Default::default();
    static ref PUBLISHERS: ArcSwap<rpds::HashTrieMap<PublisherPtr, PublisherState, ArcK>> =
        Default::default();
    static ref SUBSCRIPTIONS: ArcSwap<rpds::HashTrieMap<SubscriptionPtr, SubscriptionState, ArcK>> =
        Default::default();
    static ref SERVICES: ArcSwap<rpds::HashTrieMap<ServicePtr, ServiceState, ArcK>> =
        Default::default();
    static ref CLIENTS: ArcSwap<rpds::HashTrieMap<ClientPtr, ClientState, ArcK>> =
        Default::default();
    static ref SEND_CH: tokio::sync::mpsc::UnboundedSender<RosEvent> = {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        std::thread::spawn(|| {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("Failed to initialize Modality probe tokio runtime: {e:?}");
                    return;
                }
            };

            rt.block_on(async {
                match MessageProcessor::new().await {
                    Ok(processor) => processor.message_loop(rx).await,
                    Err(e) => {
                        eprintln!("Failed to initialize Modality probe message processor: {e:?}");
                    }
                }
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
pub struct PublisherGraphId([u8; 24]);

impl std::fmt::Display for PublisherGraphId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    ServiceEvent(ServiceEvent),
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
        pub_count: usize,
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
    CreateService {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        service_name: String,
        schema_namespace: String,
        schema_name: String,
    },
    DestroyService {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        service_name: String,
    },
    CreateClient {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        service_name: String,
        schema_namespace: String,
        schema_name: String,
    },
    DestroyClient {
        node_timeline_id: TimelineId,
        timestamp: Option<Nanoseconds>,
        service_name: String,
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
        pub_count: usize,
    ) -> RmwEvent {
        RmwEvent::CreatePublisher {
            node_timeline_id,
            timestamp: now(),
            gid,
            topic,
            schema_namespace,
            schema_name,
            pub_count,
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
            }
            | RmwEvent::DestroyNode {
                node_timeline_id, ..
            }
            | RmwEvent::CreatePublisher {
                node_timeline_id, ..
            }
            | RmwEvent::DestroyPublisher {
                node_timeline_id, ..
            }
            | RmwEvent::CreateSubscription {
                node_timeline_id, ..
            }
            | RmwEvent::DestroySubscription {
                node_timeline_id, ..
            }
            | RmwEvent::CreateService {
                node_timeline_id, ..
            }
            | RmwEvent::DestroyService {
                node_timeline_id, ..
            }
            | RmwEvent::CreateClient {
                node_timeline_id, ..
            }
            | RmwEvent::DestroyClient {
                node_timeline_id, ..
            } => node_timeline_id,
        }
    }

    pub fn timestamp(&self) -> &Option<Nanoseconds> {
        match self {
            RmwEvent::CreateNode { timestamp, .. }
            | RmwEvent::DestroyNode { timestamp, .. }
            | RmwEvent::CreatePublisher { timestamp, .. }
            | RmwEvent::DestroyPublisher { timestamp, .. }
            | RmwEvent::CreateSubscription { timestamp, .. }
            | RmwEvent::DestroySubscription { timestamp, .. }
            | RmwEvent::CreateService { timestamp, .. }
            | RmwEvent::DestroyService { timestamp, .. }
            | RmwEvent::CreateClient { timestamp, .. }
            | RmwEvent::DestroyClient { timestamp, .. } => timestamp,
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

#[derive(Debug)]
pub struct ServiceEvent {
    pub timeline_id: TimelineId,

    /// The timestamp captured at the point of our hook
    /// function. Publish and receive timestamps are available in the
    /// RMW api at some points, but not consistently enough for us to
    /// use them here.
    pub timestamp: Option<Nanoseconds>,
    pub service_name: String,
    pub event_type: ServiceEventType,

    /// The uuid of a client, in a service interaction. This is stored
    /// as 'reader_guid' in a service client, and shows up as
    /// 'writer_guid' on the service side.
    pub client_guid: [u8; 16],
    pub sequence_id: i64,
    pub kvs: Vec<(String, AttrVal)>,
}

#[derive(Debug)]
pub enum ServiceEventType {
    SendRequest,
    TakeRequest,
    SendResponse,
    TakeResponse,
}

impl ServiceEventType {
    pub fn opposite(&self) -> Self {
        match self {
            ServiceEventType::SendRequest => ServiceEventType::TakeRequest,
            ServiceEventType::TakeRequest => ServiceEventType::SendRequest,
            ServiceEventType::SendResponse => ServiceEventType::TakeResponse,
            ServiceEventType::TakeResponse => ServiceEventType::SendResponse,
        }
    }
}

impl std::fmt::Display for ServiceEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ServiceEventType::SendRequest => "send_request",
            ServiceEventType::TakeRequest => "recv_request",
            ServiceEventType::SendResponse => "send_response",
            ServiceEventType::TakeResponse => "recv_response",
        };
        f.write_str(s)
    }
}

type ServicePtr = usize;
struct ServiceState {
    schema: RosServiceSchema,
    node_timeline_id: TimelineId,
    service_name: String,
}

type ClientPtr = usize;
struct ClientState {
    schema: RosServiceSchema,
    node_timeline_id: TimelineId,
    service_name: String,
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_create_node(
    name: *const c_char,
    namespace: *const c_char,
    node_address: NodePtr,
) {
    let node_name = String::from_utf8_lossy(CStr::from_ptr(name).to_bytes()).to_string();
    let node_namespace = String::from_utf8_lossy(CStr::from_ptr(namespace).to_bytes()).to_string();

    let node_timeline_id = TimelineId::allocate();

    NODES.rcu(|nodes| {
        nodes.insert(
            node_address,
            NodeState {
                name: node_name.clone(),
                namespace: node_namespace.clone(),
                timeline_id: node_timeline_id,
            },
        )
    });

    let _ = SEND_CH.send(RmwEvent::create_node(node_namespace, node_name, node_timeline_id).into());
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_destroy_node(node_ptr: NodePtr) {
    if let Some(node_state) = NODES.load().get(&node_ptr) {
        let _ = SEND_CH.send(RmwEvent::destroy_node(node_state.timeline_id).into());
    }

    NODES.rcu(|nodes| nodes.remove(&node_ptr));
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_create_publisher(
    node_ptr: NodePtr,
    pub_ptr: *const ros::rmw::publisher,
    type_support: *const ros::rosidl::message_type_support,
    topic_name: *const c_char,
) {
    let nodes = NODES.load();
    let Some(node_state) = nodes.get(&node_ptr) else {
        return;
    };

    let topic_name_str = String::from_utf8_lossy(CStr::from_ptr(topic_name).to_bytes())
        .trim()
        .to_string();
    if CONFIG.ignored_topics.contains(&topic_name_str) {
        // shortcut return for ignored topics
        return;
    }

    let maybe_schema = RosMessageSchema::from_message_type_support(type_support);

    let mut graph_id = None;
    let mut c_gid: ros::rmw::gid = std::mem::zeroed(); // good as anything?
    if ros::rmw::get_gid_for_publisher(pub_ptr, &mut c_gid) == 0 {
        graph_id = Some(PublisherGraphId(c_gid.data));
    }

    let mut pub_count = 0;
    if let Some(message_schema) = maybe_schema {
        PUBLISHERS.rcu(|pubs| {
            pub_count = pubs.size() + 1;
            pubs.insert(
                pub_ptr as usize,
                PublisherState {
                    message_schema: message_schema.clone().flatten(&vec![]),
                    topic_name: topic_name_str.clone(),
                    node_namespace: node_state.namespace.clone(),
                    node_name: node_state.name.clone(),
                    node_timeline_id: node_state.timeline_id,
                    graph_id,
                },
            )
        });

        let _ = SEND_CH.send(
            RmwEvent::create_publisher(
                node_state.timeline_id,
                graph_id,
                topic_name_str,
                message_schema.namespace,
                message_schema.name,
                pub_count,
            )
            .into(),
        );
    }
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_destroy_publisher(pub_ptr: PublisherPtr) {
    if let Some(pub_state) = PUBLISHERS.load().get(&pub_ptr) {
        let _ = SEND_CH.send(
            RmwEvent::destroy_publisher(pub_state.node_timeline_id, pub_state.graph_id).into(),
        );
    }

    PUBLISHERS.rcu(|pubs| pubs.remove(&pub_ptr));
}

#[no_mangle]
unsafe extern "C" fn modality_before_rmw_publish(pub_ptr: PublisherPtr, message: *const c_void) {
    // Since this is a 'before' hook, the rmw impl hasn't done a sanity check yet.
    if pub_ptr == 0 || message.is_null() {
        return;
    }

    let publishers = PUBLISHERS.load();
    let Some(pub_state) = publishers.get(&pub_ptr) else {
        return;
    };

    let message_size = pub_state.message_schema.size;
    let src_message_bytes: &[u8] = std::slice::from_raw_parts(message as *const u8, message_size);

    let mut captured_messages = vec![];
    for kvs in pub_state
        .message_schema
        .interpret_potentially_compound_message(src_message_bytes)
    {
        let topic_name = pub_state.topic_name.clone();
        let node_namespace = pub_state.node_namespace.clone();
        let node_name = pub_state.node_name.clone();
        let node_timeline_id = pub_state.node_timeline_id;

        let direction = MessageDirection::Send {
            local_publisher_graph_id: pub_state.graph_id,
        };
        let captured_message = CapturedMessage {
            kvs,
            topic_name,
            node_namespace,
            node_name,
            direction,
            node_timeline_id,
        };
        captured_messages.push(captured_message);
    }

    let _called_after_dest = LAST_CAPTURED_MESSAGE
        .try_with(|lcm| {
            *lcm.borrow_mut() = Some(captured_messages);
        })
        .is_err();
}

#[repr(C)]
pub struct TimeSpec {
    tv_sec: c_long, // actually time_t
    tv_nsec: c_long,
}

#[no_mangle]
unsafe extern "C" fn modality_after_clock_gettime(clock_id: usize, timespec: *const TimeSpec) {
    // Don't print anything in here! You'll break pieces of ROS
    // that expect certain things on stdout AND stderr.

    // clock id 0 is the regular wall clock. fastdds does some
    // other calls for a different id before this one, which is
    // attached to the message.
    if clock_id != 0 {
        return;
    }

    let _ = LAST_CAPTURED_MESSAGE
        .try_with(|b| {
            if let Some(msgs) = b.borrow_mut().take() {
                for msg in msgs.into_iter() {
                    let msg_with_time = CapturedMessageWithTime {
                        msg,
                        publish_time: CapturedTime::Compound {
                            sec: (*timespec).tv_sec,
                            nsec: (*timespec).tv_nsec,
                        },
                        receive_time: None,
                    };
                    // intentionally ignore errors here. This will
                    // fail if the rx thread is dead, but in that case
                    // a message has already been printed, and this
                    // would just be belaboring the point, repeatedly.
                    let _ = SEND_CH.send(msg_with_time.into());
                }
            }
        })
        .is_err();
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_create_subscription(
    node_ptr: NodePtr,
    sub_ptr: SubscriptionPtr,
    type_support: *const ros::rosidl::message_type_support,
    topic_name: *const c_char,
) {
    let nodes = NODES.load();
    let Some(node_state) = nodes.get(&node_ptr) else {
        return;
    };

    let topic_name_str = String::from_utf8_lossy(CStr::from_ptr(topic_name).to_bytes())
        .trim()
        .to_string();
    if CONFIG.ignored_topics.contains(&topic_name_str) {
        // shortcut return for ignored topics
        return;
    }

    let Some(message_schema) = RosMessageSchema::from_message_type_support(type_support) else {
        return;
    };

    SUBSCRIPTIONS.rcu(|subs| {
        subs.insert(
            sub_ptr,
            SubscriptionState {
                message_schema: message_schema.clone().flatten(&vec![]),
                topic_name: topic_name_str.clone(),
                node_namespace: node_state.namespace.clone(),
                node_name: node_state.name.clone(),
                node_timeline_id: node_state.timeline_id,
            },
        )
    });

    let _ = SEND_CH.send(
        RmwEvent::create_subscription(
            node_state.timeline_id,
            topic_name_str,
            message_schema.namespace,
            message_schema.name,
        )
        .into(),
    );
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_destroy_subscription(sub_ptr: SubscriptionPtr) {
    if let Some(sub_state) = SUBSCRIPTIONS.load().get(&sub_ptr) {
        let _ = SEND_CH.send(
            RmwEvent::destroy_subscription(
                sub_state.node_timeline_id,
                sub_state.topic_name.clone(),
            )
            .into(),
        );
    }

    SUBSCRIPTIONS.rcu(|subs| subs.remove(&sub_ptr));
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_take_with_info(
    sub_ptr: SubscriptionPtr,
    message: *mut c_void,
    message_info: *mut ros::rmw::message_info,
) {
    let subscriptions = SUBSCRIPTIONS.load();
    let Some(sub_state) = subscriptions.get(&sub_ptr) else {
        return;
    };

    let message_size = sub_state.message_schema.size;
    let src_message_bytes: &[u8] = std::slice::from_raw_parts(message as *const u8, message_size);

    for kvs in sub_state
        .message_schema
        .interpret_potentially_compound_message(src_message_bytes)
    {
        let topic_name = sub_state.topic_name.clone();
        let node_namespace = sub_state.node_namespace.clone();
        let node_name = sub_state.node_name.clone();
        let node_timeline_id = sub_state.node_timeline_id;

        let remote_publisher_graph_id = Some(PublisherGraphId((*message_info).publisher_gid.data));
        let direction = MessageDirection::Receive {
            remote_publisher_graph_id,
        };

        let msg = CapturedMessageWithTime {
            msg: CapturedMessage {
                kvs,
                topic_name,
                node_namespace,
                node_name,
                direction,
                node_timeline_id,
            },
            publish_time: CapturedTime::SignedEpochNanos((*message_info).source_timestamp),
            receive_time: Some(CapturedTime::SignedEpochNanos(
                (*message_info).received_timestamp,
            )),
        };

        // intentionally ignore errors here. This will fail if
        // the rx thread is dead, but in that case a message
        // has already been printed, and this would just be
        // belaboring the point, repeatedly.
        let _ = SEND_CH.send(msg.into());
    }
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_create_service(
    node_ptr: NodePtr,
    service: *const ros::rmw::service,
    type_support: *const ros::rosidl::service_type_support,
    service_name: *const c_char,
) {
    let nodes = NODES.load();
    let Some(node_state) = nodes.get(&node_ptr) else {
        return;
    };

    let service_name_str = String::from_utf8_lossy(CStr::from_ptr(service_name).to_bytes())
        .trim()
        .to_string();

    let maybe_schema = RosServiceSchema::from_service_type_support(type_support);
    if let Some(service_schema) = maybe_schema {
        SERVICES.rcu(|services| {
            services.insert(
                service as usize,
                ServiceState {
                    schema: service_schema.clone(),
                    node_timeline_id: node_state.timeline_id,
                    service_name: service_name_str.clone(),
                },
            )
        });

        let _ = SEND_CH.send(
            RmwEvent::CreateService {
                node_timeline_id: node_state.timeline_id,
                timestamp: now(),
                service_name: service_name_str,
                schema_namespace: service_schema.name.clone(),
                schema_name: service_schema.namespace.clone(),
            }
            .into(),
        );
    }
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_destroy_service(destroyed_service_addr: usize) {
    if let Some(service_state) = SERVICES.load().get(&destroyed_service_addr) {
        let _ = SEND_CH.send(
            RmwEvent::DestroyService {
                node_timeline_id: service_state.node_timeline_id,
                timestamp: now(),
                service_name: service_state.service_name.clone(),
            }
            .into(),
        );
    }

    SERVICES.rcu(|services| services.remove(&destroyed_service_addr));
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_create_client(
    node_ptr: NodePtr,
    client: ClientPtr,
    type_support: *const ros::rosidl::service_type_support,
    service_name: *const c_char,
) {
    let nodes = NODES.load();
    let Some(node_state) = nodes.get(&node_ptr) else {
        return;
    };

    let service_name_str = String::from_utf8_lossy(CStr::from_ptr(service_name).to_bytes())
        .trim()
        .to_string();

    let maybe_schema = RosServiceSchema::from_service_type_support(type_support);
    if let Some(client_schema) = maybe_schema {
        CLIENTS.rcu(|clients| {
            clients.insert(
                client,
                ClientState {
                    schema: client_schema.clone(),
                    node_timeline_id: node_state.timeline_id,
                    service_name: service_name_str.clone(),
                },
            )
        });

        let _ = SEND_CH.send(
            RmwEvent::CreateClient {
                node_timeline_id: node_state.timeline_id,
                timestamp: now(),
                service_name: service_name_str,
                schema_namespace: client_schema.name.clone(),
                schema_name: client_schema.namespace.clone(),
            }
            .into(),
        );
    }
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_destroy_client(destroyed_client_addr: usize) {
    if let Some(client_state) = CLIENTS.load().get(&destroyed_client_addr) {
        let _ = SEND_CH.send(
            RmwEvent::DestroyClient {
                node_timeline_id: client_state.node_timeline_id,
                timestamp: now(),
                service_name: client_state.service_name.clone(),
            }
            .into(),
        );
    }

    CLIENTS.rcu(|clients| clients.remove(&destroyed_client_addr));
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_send_request(
    client: *const ros::rmw::client,
    ros_request: *const c_void,
    sequence_id: i64,
) {
    let clients = CLIENTS.load();
    let Some(client_state) = clients.get(&(client as usize)) else {
        return;
    };

    // Why, you may ask, are we using the 'reader_guid' for a thing that is obviously writing? Becuase that's what
    // fastrtps does, and observed behavior agrees. Couldn't tell you why.
    let client_guid = rtps_client_reader_guid(client);

    let request_size = client_state.schema.request_members.size;
    let request_bytes: &[u8] = std::slice::from_raw_parts(ros_request as *const u8, request_size);
    let kvs = client_state
        .schema
        .request_members
        .interpret_single_message(request_bytes);

    let service_event = ServiceEvent {
        timeline_id: client_state.node_timeline_id,
        timestamp: now(),
        service_name: client_state.service_name.clone(),
        event_type: ServiceEventType::SendRequest,
        client_guid,
        sequence_id,
        kvs,
    };

    let _ = SEND_CH.send(RosEvent::ServiceEvent(service_event));
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_take_request(
    service: ServicePtr,
    request_header: *const ros::rmw::service_info,
    ros_request: *const c_void,
) {
    let services = SERVICES.load();
    let Some(service_state) = services.get(&service) else {
        return;
    };

    let request_size = service_state.schema.request_members.size;
    let request_bytes: &[u8] = std::slice::from_raw_parts(ros_request as *const u8, request_size);
    let kvs = service_state
        .schema
        .request_members
        .interpret_single_message(request_bytes);

    let client_guid_i8: [i8; 16] = (*request_header).request_id.writer_guid;
    let client_guid: [u8; 16] = std::mem::transmute(client_guid_i8);
    let sequence_id = (*request_header).request_id.sequence_number;

    let _ = SEND_CH.send(RosEvent::ServiceEvent(ServiceEvent {
        timeline_id: service_state.node_timeline_id,
        timestamp: now(),
        service_name: service_state.service_name.clone(),
        event_type: ServiceEventType::TakeRequest,
        client_guid,
        sequence_id,
        kvs,
    }));
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_send_response(
    service: ServicePtr,
    request_header: *const ros::rmw::request_id,
    ros_response: *const c_void,
) {
    let services = SERVICES.load();
    let Some(service_state) = services.get(&service) else {
        return;
    };

    let response_size = service_state.schema.request_members.size;
    let response_bytes: &[u8] =
        std::slice::from_raw_parts(ros_response as *const u8, response_size);
    let kvs = service_state
        .schema
        .response_members
        .interpret_single_message(response_bytes);

    let client_guid_i8: [i8; 16] = (*request_header).writer_guid;
    let client_guid: [u8; 16] = std::mem::transmute(client_guid_i8);
    let sequence_id = (*request_header).sequence_number;

    let _ = SEND_CH.send(RosEvent::ServiceEvent(ServiceEvent {
        timeline_id: service_state.node_timeline_id,
        timestamp: now(),
        service_name: service_state.service_name.clone(),
        event_type: ServiceEventType::SendResponse,
        client_guid,
        sequence_id,
        kvs,
    }));
}

#[no_mangle]
unsafe extern "C" fn modality_after_rmw_take_response(
    client: *const ros::rmw::client,
    request_header: *const ros::rmw::service_info,
    ros_response: *const c_void,
) {
    let clients = CLIENTS.load();
    let Some(client_state) = clients.get(&(client as usize)) else {
        return;
    };

    let client_guid = rtps_client_reader_guid(client);
    let sequence_id = (*request_header).request_id.sequence_number;

    let response_size = client_state.schema.response_members.size;
    let response_bytes: &[u8] =
        std::slice::from_raw_parts(ros_response as *const u8, response_size);
    let kvs = client_state
        .schema
        .response_members
        .interpret_single_message(response_bytes);

    let service_event = ServiceEvent {
        timeline_id: client_state.node_timeline_id,
        timestamp: now(),
        service_name: client_state.service_name.clone(),
        event_type: ServiceEventType::TakeResponse,
        client_guid,
        sequence_id,
        kvs,
    };

    let _ = SEND_CH.send(RosEvent::ServiceEvent(service_event));
}

// We need to dig into the fast-rtps (fastdds) internal state to pull out the client id, since it's not exposed via the RMW apis.
unsafe fn rtps_client_reader_guid(client: *const ros::rmw::client) -> [u8; 16] {
    let fastrtps_custom_client_info = (*client).data as *const ros::fastrtps::CustomClientInfo;
    let reader_guid = &(*fastrtps_custom_client_info).reader_guid_;

    // fastrtps stores its id as a prefix followed by an entityid, we need to glue thosee together to get the "writer_guid"
    // value that we  observe on the service side.
    let mut full_guid = [0u8; 16];
    full_guid[0..=11].copy_from_slice(&reader_guid.guidPrefix.value);
    full_guid[12..16].copy_from_slice(&reader_guid.entityId.value);
    full_guid
}
