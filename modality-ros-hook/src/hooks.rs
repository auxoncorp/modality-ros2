use crate::{
    interop::{self, rmw_get_gid_for_publisher, RmwGid, RmwMessageInfo},
    message_processor::MessageProcessor,
    FlatRosMessageSchema, RosMessageSchema,
};
use arc_swap::ArcSwap;
use archery::ArcK;
use lazy_static::lazy_static;
use modality_api::{AttrVal, Nanoseconds};

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
    pub static LAST_CAPTURED_MESSAGE: RefCell<Option<CapturedMessage<'static>>>  = RefCell::new(None);
}

// TODO destroy node/publisher/subscription is a thing... do we care to track those?

lazy_static! {
    static ref NODES: ArcSwap<rpds::List<NodeState, ArcK>> =
        ArcSwap::from_pointee(rpds::List::<NodeState, ArcK>::new_with_ptr_kind());

    static ref PUBLISHERS: ArcSwap<rpds::List<PublisherState, ArcK>> =
        ArcSwap::from_pointee(rpds::List::<PublisherState, ArcK>::new_with_ptr_kind());

    static ref SUBSCRIPTIONS: ArcSwap<rpds::List<SubscriptionState, ArcK>> =
        ArcSwap::from_pointee(rpds::List::<SubscriptionState, ArcK>::new_with_ptr_kind());


    static ref SEND_CH: tokio::sync::mpsc::UnboundedSender<CapturedMessageWithTime<'static>> = {
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
    address: NodePtr,
    node_name: String,
    node_namespace: String,
}

/// The unique identifier (gid) of a publisher.
#[derive(Copy, Clone, Debug)]
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
    address: PublisherPtr,
    message_schema: FlatRosMessageSchema,
    topic_name: String,
    node_namespace: String,
    node_name: String,
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

type SubscriptionAddress = usize;
struct SubscriptionState {
    address: SubscriptionAddress,
    message_schema: FlatRosMessageSchema,
    topic_name: String,
    node_namespace: String,
    node_name: String,
}

#[derive(Debug)]
// 'p is the lifetime of the PublisherState entry that these are referring into.
pub struct CapturedMessage<'p> {
    pub kvs: Vec<(String, AttrVal)>,
    pub topic_name: &'p str,
    pub node_namespace: &'p str,
    pub node_name: &'p str,
    pub direction: MessageDirection,
}

#[derive(Debug)]
pub struct CapturedMessageWithTime<'p> {
    pub msg: CapturedMessage<'p>,
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

        NODES.rcu(|nodes| {
            nodes.push_front(NodeState {
                address: node_address, node_name: node_name.clone(), node_namespace: node_namespace.clone()})
        });

        node_address
    }
}

redhook::hook! {
    unsafe fn rmw_create_publisher(node: NodePtr, type_support: *const interop::RosIdlMessageTypeSupportT,
                                   topic_name: *const c_char, qos_policies: *const c_void,
                                   publisher_options: *const c_void) -> PublisherPtr
        => modality_hook_rmw_create_publisher
    {
        let topic_name_str = String::from_utf8_lossy(CStr::from_ptr(topic_name).to_bytes()).to_string();
        let mut maybe_schema = RosMessageSchema::from_c(type_support);

        if let Some(node_state) = NODES.load().iter().find(|ns| ns.address == node) {
            let publisher_address = redhook::real!(rmw_create_publisher)
                (node, type_support, topic_name, qos_policies, publisher_options);

            let mut graph_id = None;
            let mut c_gid = RmwGid::default();
            if rmw_get_gid_for_publisher(publisher_address, &mut c_gid) == 0 {
                graph_id = Some(PublisherGraphId(c_gid.data));
            }

            if let Some(message_schema) = maybe_schema.take() {
                PUBLISHERS.rcu(|list| {
                    list.push_front(
                        PublisherState {
                            address: publisher_address,
                            message_schema: message_schema.clone().flatten(&vec![]),
                            topic_name: topic_name_str.clone(),
                            node_namespace: node_state.node_namespace.clone(),
                            node_name: node_state.node_name.clone(),
                            graph_id
                        }
                    )
                });
            }
            publisher_address
        } else {
            // If someone tries to createa publisher against an
            // uninitialized node, I guess? This is probably an error,
            // and really shouldn't be happening.
            redhook::real!(rmw_create_publisher)
                (node, type_support, topic_name, qos_policies, publisher_options)
        }
    }
}

redhook::hook! {
    unsafe fn rmw_publish(publisher_address: PublisherPtr, message: *const c_void, allocation: *const c_void) -> c_int
        => modality_hook_rmw_publish
    {
        if let Some(pub_state) = PUBLISHERS.load().iter().find(|ps| ps.address == publisher_address) {
            let message_size = pub_state.message_schema.size;
            let src_message_bytes: &[u8] = std::slice::from_raw_parts(message as *const u8, message_size);
            let mut kvs = vec![];
            pub_state.message_schema.interpret_message(None, src_message_bytes, &mut kvs);

            // cast away the lifetime; these are in fact valid for
            // 'static because the strings are in a global registry,
            // which is a grow-only persistent data structure..
            let topic_name: &'static str = &*(pub_state.topic_name.as_str() as *const _);
            let node_namespace: &'static str = &*(pub_state.node_namespace.as_str() as *const _);
            let node_name: &'static str = &*(pub_state.node_name.as_str() as *const _);
            // TODO ref here
            let direction = MessageDirection::Send { local_publisher_graph_id: pub_state.graph_id.clone() };
            let captured_message = CapturedMessage { kvs, topic_name, node_namespace, node_name, direction };
            let _called_after_dest = LAST_CAPTURED_MESSAGE.try_with(|lcm| {
                *lcm.borrow_mut() = Some(captured_message);
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

        // 0 means success
        if res == 0 {
            let _called_after_dest = LAST_CAPTURED_MESSAGE.try_with(|b| {
                if let Some(msg) = b.borrow_mut().take() {
                    let msg_with_time = CapturedMessageWithTime {
                        msg,
                        publish_time: CapturedTime::Compound { sec: (*timespec).tv_sec, nsec: (*timespec).tv_nsec },
                        receive_time: None
                    };

                    // intentionally ignore errors here. This will
                    // fail if the rx thread is dead, but in that case
                    // a message has already been printed, and this
                    // would just be belaboring the point, repeatedly.
                    let _ = SEND_CH.send(msg_with_time);
                }
            }).is_err();
        }

        res
    }
}

redhook::hook! {
    unsafe fn rmw_create_subscription(node: NodePtr, type_support: *const interop::RosIdlMessageTypeSupportT,
                                      topic_name: *const c_char, qos_policies: *const c_void,
                                      subscription_options: *const c_void) -> SubscriptionAddress
        => modality_hook_rmw_create_subscription

    {
        let topic_name_str = String::from_utf8_lossy(CStr::from_ptr(topic_name).to_bytes()).to_string();
        let mut maybe_schema = RosMessageSchema::from_c(type_support);

        if let Some(node_state) = NODES.load().iter().find(|ns| ns.address == node) {
            let subscription_address = redhook::real!(rmw_create_subscription)
                (node, type_support, topic_name, qos_policies, subscription_options);

            if let Some(message_schema) = maybe_schema.take() {
                SUBSCRIPTIONS.rcu(|list| {
                    list.push_front(
                        SubscriptionState {
                            address: subscription_address,
                            message_schema: message_schema.clone().flatten(&vec![]),
                            topic_name: topic_name_str.clone(),
                            node_namespace: node_state.node_namespace.clone(),
                            node_name: node_state.node_name.clone(),
                        }
                    )
                });
            }
            subscription_address
        } else {
            redhook::real!(rmw_create_subscription)
                (node, type_support, topic_name, qos_policies, subscription_options)
        }
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
            if let Some(sub_state) = SUBSCRIPTIONS.load().iter().find(|sub| sub.address == subscription_address) {
                let message_size = sub_state.message_schema.size;
                let src_message_bytes: &[u8] = std::slice::from_raw_parts(message as *const u8, message_size);
                let mut kvs = vec![];
                sub_state.message_schema.interpret_message(None, src_message_bytes, &mut kvs);

                // cast away the lifetime; these are in fact valid for
                // 'static because the strings are in a global registry,
                // which is a grow-only persistent data structure.
                let topic_name: &'static str = &*(sub_state.topic_name.as_str() as *const _);
                let node_namespace: &'static str = &*(sub_state.node_namespace.as_str() as *const _);
                let node_name: &'static str = &*(sub_state.node_name.as_str() as *const _);

                let remote_publisher_graph_id = Some(PublisherGraphId((*message_info).publisher_gid.data));
                let direction = MessageDirection::Receive { remote_publisher_graph_id };

                let msg = CapturedMessageWithTime {
                    msg: CapturedMessage { kvs, topic_name, node_namespace, node_name, direction},
                    publish_time: CapturedTime::SignedEpochNanos((*message_info).source_timestamp),
                    receive_time: Some(CapturedTime::SignedEpochNanos((*message_info).received_timestamp)),
                };

                // intentionally ignore errors here. This will fail if
                // the rx thread is dead, but in that case a message
                // has already been printed, and this would just be
                // belaboring the point, repeatedly.
                let _ = SEND_CH.send(msg);
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
