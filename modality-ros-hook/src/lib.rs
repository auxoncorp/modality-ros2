use auxon_sdk::api::{AttrVal, BigInt};
use config::CONFIG;
use core::slice;
use itertools::Itertools;
use std::{
    borrow::Cow,
    ffi::{c_char, c_void, CStr},
};

mod config;
mod hooks;
mod message_processor;
mod ros;

#[derive(Debug, Clone)]
pub struct RosMessageSchema {
    pub namespace: String,
    pub name: String,
    pub size: usize,
    pub members: Vec<RosMessageMemberSchema>,
}

#[derive(Debug, Clone)]
pub enum RosMessageMemberSchema {
    Scalar {
        name: String,
        type_id: ros::rosidl::typesupport_introspection::field_type,
        offset: usize,
    },
    Array {
        offset: usize,
        item_schema: Box<RosMessageMemberSchema>,
        array_size: usize,
        is_upper_bound: bool,
        size_function: Option<unsafe extern "C" fn(*const c_void) -> usize>,
        get_function: Option<unsafe extern "C" fn(*const c_void, usize) -> *const c_void>,
    },
    NestedMessage {
        name: String,
        offset: usize,
        schema: RosMessageSchema,
    },
}

#[derive(Debug, Clone)]
pub struct FlatRosMessageSchema {
    namespace: String,
    name: String,
    size: usize,
    members: Vec<FlatRosMessageMemberSchema>,
}

#[derive(Debug, Clone)]
pub enum FlatRosMessageMemberSchema {
    Scalar(ScalarMemberSchema),
    ScalarArray(ScalarArrayMemberSchema),
    MessageSequence(MessageSequenceMemberSchema),
}

#[derive(Debug, Clone)]
pub struct ScalarMemberSchema {
    pub key: String,
    pub type_id: ros::rosidl::typesupport_introspection::field_type,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct ScalarArrayMemberSchema {
    pub key: String,
    pub type_id: ros::rosidl::typesupport_introspection::field_type,
    pub offset_in_item_slice: usize,
    pub array_size: usize,
    pub is_upper_bound: bool,
    pub size_function: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    pub get_function: Option<unsafe extern "C" fn(*const c_void, usize) -> *const c_void>,
}

#[derive(Debug, Clone)]
pub struct MessageSequenceMemberSchema {
    pub key: String,
    pub offset: usize,
    pub message_schema: FlatRosMessageSchema,
    pub array_size: usize,
    pub is_upper_bound: bool,
    pub size_function: Option<unsafe extern "C" fn(*const c_void) -> usize>,
    pub get_function: Option<unsafe extern "C" fn(*const c_void, usize) -> *const c_void>,
}

impl RosMessageSchema {
    #[allow(clippy::if_same_then_else)]
    unsafe fn from_message_type_support(
        type_support: *const ros::rosidl::message_type_support,
    ) -> Option<Self> {
        let ts = ros::rmw::get_typesupport(type_support);

        let ts_id = CStr::from_ptr((*ts).typesupport_identifier);
        let c_schema: *const ros::rosidl::typesupport_introspection::MessageMembers =
            if ts_id == ros::rosidl::typesupport_introspection::C_IDENTIFIER {
                std::mem::transmute((*ts).data)
            } else if ts_id == ros::rosidl::typesupport_introspection::CPP_IDENTIFIER {
                // they're actually the same exact memory layout
                std::mem::transmute((*ts).data)
            } else {
                eprintln!(
                    "Modality probe: unknown typesupport type: {}",
                    ros::debug_c_str((*ts).typesupport_identifier)
                );
                return None;
            };

        let members = convert_c_message_members_to_schemas(c_schema)?;

        Some(RosMessageSchema {
            namespace: String::from_utf8_lossy(
                CStr::from_ptr((*c_schema).message_namespace_).to_bytes(),
            )
                .to_string(),
            name: String::from_utf8_lossy(CStr::from_ptr((*c_schema).message_name_).to_bytes())
                .to_string(),
            size: (*c_schema).size_of_,
            members,
        })
    }

    pub fn flatten(self, prefix: &Vec<String>) -> FlatRosMessageSchema {
        FlatRosMessageSchema::from_nested_members(
            self.namespace,
            self.name,
            self.size,
            self.members,
            prefix,
        )
    }
}

unsafe fn convert_c_message_members_to_schemas(
    c_schema: *const ros::rosidl::typesupport_introspection::MessageMembers,
) -> Option<Vec<RosMessageMemberSchema>> {
    let c_members: &[ros::rosidl::typesupport_introspection::MessageMember] =
        std::slice::from_raw_parts((*c_schema).members_, (*c_schema).member_count_ as usize);
    let mut members = vec![];
    for c_member in c_members {
        let name = String::from_utf8_lossy(CStr::from_ptr(c_member.name_).to_bytes()).to_string();
        let type_id = c_member.type_id_ as u32;
        let offset = c_member.offset_ as usize;

        let member = if c_member.is_array_ {
            let item_schema = if type_id
                == ros::rosidl::typesupport_introspection::field_types::ROS_TYPE_MESSAGE
            {
                Box::new(RosMessageMemberSchema::NestedMessage {
                    name,
                    offset,
                    schema: RosMessageSchema::from_message_type_support(c_member.members_)?,
                })
            } else {
                Box::new(RosMessageMemberSchema::Scalar {
                    name,
                    type_id,
                    offset,
                })
            };
            RosMessageMemberSchema::Array {
                item_schema,
                offset,
                array_size: c_member.array_size_,
                is_upper_bound: c_member.is_upper_bound_,
                size_function: c_member.size_function,
                get_function: c_member.get_const_function,
            }
        } else if type_id == ros::rosidl::typesupport_introspection::field_types::ROS_TYPE_MESSAGE {
            RosMessageMemberSchema::NestedMessage {
                name,
                offset,
                schema: RosMessageSchema::from_message_type_support(c_member.members_)?,
            }
        } else {
            RosMessageMemberSchema::Scalar {
                name,
                type_id,
                offset,
            }
        };
        members.push(member);
    }

    Some(members)
}

impl RosMessageMemberSchema {
    pub fn name(&self) -> &str {
        match self {
            RosMessageMemberSchema::Scalar { name, .. } => name,
            RosMessageMemberSchema::Array { item_schema, .. } => item_schema.name(),
            RosMessageMemberSchema::NestedMessage { name, .. } => name,
        }
    }

    pub fn flatten(
        self,
        prefix: &Vec<String>,
        initial_offset: usize,
        target: &mut Vec<FlatRosMessageMemberSchema>,
    ) {
        match self {
            RosMessageMemberSchema::Scalar {
                name,
                type_id,
                offset,
            } => {
                let mut name_segs = prefix.clone();
                name_segs.push(name);
                target.push(FlatRosMessageMemberSchema::Scalar(ScalarMemberSchema {
                    key: name_segs.join("."),
                    type_id,
                    offset: initial_offset + offset,
                }))
            }
            RosMessageMemberSchema::Array {
                item_schema,
                array_size,
                is_upper_bound,
                size_function,
                get_function,
                offset: array_offset,
            } => match *item_schema {
                RosMessageMemberSchema::Scalar {
                    name,
                    type_id,
                    offset: scalar_offset,
                } => {
                    let mut name_segs = prefix.clone();
                    name_segs.push(name);
                    target.push(FlatRosMessageMemberSchema::ScalarArray(
                        ScalarArrayMemberSchema {
                            key: name_segs.join("."),
                            type_id,
                            offset_in_item_slice: scalar_offset,
                            array_size,
                            is_upper_bound,
                            size_function,
                            get_function,
                        },
                    ))
                }
                RosMessageMemberSchema::NestedMessage {
                    name,
                    // This offset duplicates the offset found in the parent array
                    offset: _,
                    schema,
                } => {
                    let mut name_segs = prefix.clone();
                    name_segs.push(name);

                    // flatten the internal msg with no prefix; it is
                    // added when emitting the data
                    let message_schema = schema.flatten(&vec![]);

                    target.push(FlatRosMessageMemberSchema::MessageSequence(
                        MessageSequenceMemberSchema {
                            key: name_segs.join("."),
                            offset: initial_offset + array_offset,
                            message_schema,
                            array_size,
                            is_upper_bound,
                            size_function,
                            get_function,
                        },
                    ))
                }
                RosMessageMemberSchema::Array { .. } => {
                    unreachable!()
                }
            },
            RosMessageMemberSchema::NestedMessage {
                name,
                offset,
                schema,
            } => {
                let mut prefix = prefix.clone();
                prefix.push(name);
                for member in schema.members {
                    member.flatten(&prefix, initial_offset + offset, target);
                }
            }
        }
    }
}

impl FlatRosMessageSchema {
    fn from_nested_members(
        namespace: String,
        name: String,
        size: usize,
        members: Vec<RosMessageMemberSchema>,
        prefix: &Vec<String>,
    ) -> Self {
        let mut flat_members: Vec<FlatRosMessageMemberSchema> = vec![];
        for member in members.into_iter() {
            member.flatten(prefix, 0, &mut flat_members);
        }

        Self {
            namespace,
            name,
            size,
            members: flat_members,
        }
    }

    fn interpret_single_message<'a>(&'a self, message: &'a [u8]) -> Vec<(String, AttrVal)> {
        let mut kvs = vec![
            (
                "event.ros.schema.namespace".to_string(),
                AttrVal::String(Cow::Owned(self.namespace.clone())),
            ),
            (
                "event.ros.schema.name".to_string(),
                AttrVal::String(Cow::Owned(self.name.clone())),
            ),
        ];

        self.interpret_message_internal(None, message, &mut kvs);

        if let Some(msg) = extract_log_message(self, &kvs) {
            if let Some((_, v)) = kvs
                .iter_mut()
                .find(|(k, _v)| k == "name" || k == "event.name")
            {
                *v = msg;
            } else {
                kvs.push(("event.name".to_string(), msg));
            }
        }
        kvs
    }

    fn interpret_potentially_compound_message<'a>(
        &'a self,
        message: &'a [u8],
    ) -> impl Iterator<Item = Vec<(String, AttrVal)>> + 'a {
        if (self.namespace == "diagnostic_msgs::msg" || self.namespace == "diagnostic_msgs__msg")
            && self.name == "DiagnosticArray"
        {
            itertools::Either::Left(self.interepret_as_diagnostic_array(message))
        } else {
            let mut kvs = vec![
                (
                    "event.ros.schema.namespace".to_string(),
                    AttrVal::String(Cow::Owned(self.namespace.to_owned())),
                ),
                (
                    "event.ros.schema.name".to_string(),
                    AttrVal::String(Cow::Owned(self.name.to_owned())),
                ),
            ];

            self.interpret_message_internal(None, message, &mut kvs);

            if let Some(msg) = extract_log_message(self, &kvs) {
                if let Some((_, v)) = kvs
                    .iter_mut()
                    .find(|(k, _v)| k == "name" || k == "event.name")
                {
                    *v = msg;
                } else {
                    kvs.push(("event.name".to_string(), msg));
                }
            }

            itertools::Either::Right(std::iter::once(kvs))
        }
    }

    fn interpret_message_internal(
        &self,
        prefix: Option<&str>,
        message: &[u8],
        kvs: &mut Vec<(String, AttrVal)>,
    ) {
        for member in self.members.iter() {
            let _ = member.interpret_message(prefix, message, kvs);
        }
    }

    /// Special handling for the 'DiagnosticArray' type; it has a bunch of 'status'
    /// items in an array, and we want each of those to be emitted as a top-level event.
    fn interepret_as_diagnostic_array<'a>(
        &'a self,
        message: &'a [u8],
    ) -> impl Iterator<Item = Vec<(String, AttrVal)>> + 'a {
        let mut common_kvs = vec![
            (
                "event.ros.schema.namespace".to_string(),
                AttrVal::String(Cow::Owned(self.namespace.to_owned())),
            ),
            (
                "event.ros.schema.name".to_string(),
                AttrVal::String(Cow::Owned(self.name.to_owned())),
            ),
        ];
        for member in self.members.iter() {
            // Repeat the members outside of the status field for every event
            if as_status_field(member).is_none() {
                let _ = member.interpret_message(None, message, &mut common_kvs);
            }
        }

        // We're going to emit a whole event for each item in the status array
        let mut events = vec![];
        let status_prefix = None;

        if let Some(status_schema) = self.members.iter().find_map(as_status_field) {
            status_schema.for_each_item(message, |_, status_item_slice| {
                let mut kvs = common_kvs.clone();

                // Take the members of this particular status item.
                for item_field_schema in status_schema.message_schema.members.iter() {
                    if let Some(values_schema) = as_values_field(item_field_schema) {
                        values_schema.for_each_item(status_item_slice, |_, kv_item_slice| {
                            let mut local_attr_kvs = vec![];
                            values_schema.message_schema.interpret_message_internal(
                                None,
                                kv_item_slice,
                                &mut local_attr_kvs,
                            );

                            if let (Some((_, k)), Some((_, v))) = (
                                local_attr_kvs.iter().find(|t| t.0 == "key"),
                                local_attr_kvs.iter().find(|t| t.0 == "value"),
                            ) {
                                let ks = normalize_string_for_attr_key(k.to_string());
                                kvs.push((ks.clone(), v.clone()));
                                kvs.push((
                                    format!("{ks}.key"),
                                    AttrVal::String(Cow::Owned(k.to_string())),
                                ));
                            }

                            Some(())
                        });
                    } else {
                        item_field_schema.interpret_message(
                            status_prefix,
                            status_item_slice,
                            &mut kvs,
                        )?;
                    }
                }

                events.push(kvs);
                Some(())
            });
        } else {
            // This could happen if the schema changes. I guess just
            // emit a single event with the common attrs, which should
            // include any new stuff we didn't special case.
            events.push(common_kvs);
        }

        events.into_iter()
    }
}

#[derive(Debug, Clone)]
pub struct RosServiceSchema {
    pub namespace: String,
    pub name: String,
    pub request_members: FlatRosMessageSchema,
    pub response_members: FlatRosMessageSchema,
}

impl RosServiceSchema {
    unsafe fn from_service_type_support(
        type_support: *const ros::rosidl::service_type_support,
    ) -> Option<Self> {
        let ts = ros::rmw::get_service_typesupport(type_support);

        let ts_id = CStr::from_ptr((*ts).typesupport_identifier);
        let c_schema: *const ros::rosidl::typesupport_introspection::ServiceMembers =
            if ts_id == ros::rosidl::typesupport_introspection::C_IDENTIFIER {
                std::mem::transmute((*ts).data)
            } else if ts_id == ros::rosidl::typesupport_introspection::CPP_IDENTIFIER {
                // they're actually the same exact memory layout
                std::mem::transmute((*ts).data)
            } else {
                eprintln!(
                    "Modality probe: unknown typesupport type: {}",
                    ros::debug_c_str((*ts).typesupport_identifier)
                );
                return None;
            };
        let namespace =
            String::from_utf8_lossy(CStr::from_ptr((*c_schema).service_namespace_).to_bytes())
                .to_string();
        let name = String::from_utf8_lossy(CStr::from_ptr((*c_schema).service_name_).to_bytes())
            .to_string();
        let request_members = convert_c_message_members_to_schemas((*c_schema).request_members_)?;
        let request_size = (*(*c_schema).request_members_).size_of_;

        let response_members = convert_c_message_members_to_schemas((*c_schema).response_members_)?;
        let response_size = (*(*c_schema).response_members_).size_of_;

        Some(RosServiceSchema {
            namespace: namespace.clone(),
            name: name.clone(),
            request_members: FlatRosMessageSchema::from_nested_members(
                namespace.clone(),
                name.clone(),
                request_size,
                request_members,
                &vec![],
            ),
            response_members: FlatRosMessageSchema::from_nested_members(
                namespace,
                name,
                response_size,
                response_members,
                &vec![],
            ),
        })
    }
}

/// Throw away anything non-alphanumeric. Join strings of alphanumeric
/// characters together with underscores. Lowercase the whole thing.
fn normalize_string_for_attr_key(s: String) -> String {
    let mut parts = s.split(|c: char| !c.is_alphanumeric());
    parts.join("_").to_lowercase()
}

fn as_status_field(member: &FlatRosMessageMemberSchema) -> Option<&MessageSequenceMemberSchema> {
    match member {
        FlatRosMessageMemberSchema::MessageSequence(
            seq @ MessageSequenceMemberSchema {
                key,
                message_schema:
                    FlatRosMessageSchema {
                        namespace, name, ..
                    },
                ..
            },
        ) if key == "status"
            && (namespace == "diagnostic_msgs::msg" || namespace == "diagnostic_msgs__msg")
            && name == "DiagnosticStatus" =>
        {
            Some(seq)
        }
        _ => None,
    }
}

fn as_values_field(member: &FlatRosMessageMemberSchema) -> Option<&MessageSequenceMemberSchema> {
    match member {
        FlatRosMessageMemberSchema::MessageSequence(
            seq @ MessageSequenceMemberSchema {
                key,
                message_schema:
                    FlatRosMessageSchema {
                        namespace, name, ..
                    },
                ..
            },
        ) if key == "values"
            && (namespace == "diagnostic_msgs::msg" || namespace == "diagnostic_msgs__msg")
            && name == "KeyValue" =>
        {
            Some(seq)
        }
        _ => None,
    }
}

fn extract_log_message(
    schema: &FlatRosMessageSchema,
    kvs: &[(String, AttrVal)],
) -> Option<AttrVal> {
    if (schema.namespace == "rcl_interfaces__msg" || schema.namespace == "rcl_interfaces::msg")
        && schema.name == "Log"
    {
        if let Some((_, msg)) = kvs.iter().find(|(k, _)| k == "msg") {
            return Some(msg.clone());
        }
    }

    None
}

impl FlatRosMessageMemberSchema {
    fn interpret_message(
        &self,
        prefix: Option<&str>,
        message: &[u8],
        kvs: &mut Vec<(String, AttrVal)>,
    ) -> Option<()> {
        match self {
            FlatRosMessageMemberSchema::Scalar(s) => s.interpret_message(prefix, message, kvs),
            FlatRosMessageMemberSchema::ScalarArray(s) => s.interpret_message(prefix, message, kvs),
            FlatRosMessageMemberSchema::MessageSequence(s) => {
                s.interpret_message(prefix, message, kvs)
            }
        }
    }
}

impl ScalarMemberSchema {
    fn interpret_message(
        &self,
        prefix: Option<&str>,
        message: &[u8],
        kvs: &mut Vec<(String, AttrVal)>,
    ) -> Option<()> {
        let ScalarMemberSchema {
            key,
            type_id,
            offset,
        } = &self;

        if let Some(val_slice) = message.get(*offset..) {
            let key = if let Some(p) = prefix {
                format!("{p}.{key}")
            } else {
                key.clone()
            };

            let val = unsafe { ros_to_attr_val(*type_id, val_slice.as_ptr())? };

            kvs.push((key, val));
        }

        Some(())
    }
}

impl ScalarArrayMemberSchema {
    fn interpret_message(
        &self,
        prefix: Option<&str>,
        message: &[u8],
        kvs: &mut Vec<(String, AttrVal)>,
    ) -> Option<()> {
        let ScalarArrayMemberSchema {
            key,
            array_size,
            is_upper_bound: _,
            size_function: _,
            get_function,
            type_id,
            offset_in_item_slice,
        } = &self;

        unsafe {
            if let Some(slice) = message.get(*offset_in_item_slice..) {
                let scalar_array_ptr = (slice).as_ptr() as *const c_void;

                let seq: *const ros::rosidl::runtime::ByteSequence =
                    std::mem::transmute(slice.as_ptr());

                // This *SHOULD* work. I don't know why it doesn't.
                // let scalar_array_len = ((*size_function)?)(scalar_array_ptr);

                let scalar_array_len = if *array_size != 0 {
                    *array_size
                } else {
                    (*seq).size
                };

                if scalar_array_len > CONFIG.max_array_len {
                    return Some(());
                }

                let get_function = (*get_function)?;

                for i in 0..scalar_array_len {
                    let item_key = if let Some(p) = prefix {
                        format!("{p}.{key}.{i}")
                    } else {
                        format!("{key}.{i}")
                    };

                    let item_ptr = get_function(scalar_array_ptr, i);
                    let val = ros_to_attr_val(*type_id, item_ptr as *const u8)?;
                    kvs.push((item_key, val));
                }
            }

            Some(())
        }
    }
}

impl MessageSequenceMemberSchema {
    fn interpret_message(
        &self,
        prefix: Option<&str>,
        message: &[u8],
        kvs: &mut Vec<(String, AttrVal)>,
    ) -> Option<()> {
        use std::io::Write;
        self.for_each_item(message, |i, item_slice| {
            let item_key = if let Some(p) = prefix {
                format!("{p}.{}.{i}", self.key)
            } else {
                format!("{}.{i}", self.key)
            };

            //println!("message sequence member schema: {item_key}");
            std::io::stdout().flush().unwrap();

            for item_field_schema in self.message_schema.members.iter() {
                item_field_schema.interpret_message(Some(&item_key), item_slice, kvs)?;
            }

            Some(())
        })
    }

    /// Call `f` once for each message in the sequence. It will be
    /// passed a 'message' slice at the the proper offset.
    fn for_each_item(
        &self,
        message: &[u8],
        mut f: impl FnMut(usize, &[u8]) -> Option<()>,
    ) -> Option<()> {
        unsafe {
            if let Some(slice) = message.get(self.offset..) {
                let seq: *const ros::rosidl::runtime::ByteSequence =
                    std::mem::transmute(slice.as_ptr());
                let msg_array_len = ((self.size_function)?)(seq as _);
                let get_function = self.get_function?;

                for i in 0..msg_array_len {
                    let item_ptr = get_function(slice.as_ptr() as _, i);
                    let item_slice: &[u8] =
                        slice::from_raw_parts(item_ptr as _, self.message_schema.size);
                    f(i, item_slice)?;
                }
            }
            Some(())
        }
    }
}

unsafe fn ros_to_attr_val(
    type_id: ros::rosidl::typesupport_introspection::field_type,
    ptr: *const u8,
) -> Option<AttrVal> {
    use ros::rosidl::typesupport_introspection::field_types::*;
    Some(match type_id {
        ROS_TYPE_STRING => {
            let rt_str: *const ros::rosidl::runtime::String = std::mem::transmute(ptr);
            AttrVal::String(Cow::Owned((*rt_str).as_string()))
        }
        ROS_TYPE_WSTRING => {
            let rt_str: *const ros::rosidl::runtime::U16String = std::mem::transmute(ptr);
            AttrVal::String(Cow::Owned((*rt_str).as_string()?))
        }
        ROS_TYPE_FLOAT => AttrVal::Float(
            (f32::from_ne_bytes(slice::from_raw_parts(ptr, 4).try_into().ok()?) as f64).into(),
        ),
        ROS_TYPE_DOUBLE => AttrVal::Float(
            f64::from_ne_bytes(slice::from_raw_parts(ptr, 8).try_into().ok()?).into(),
        ),
        ROS_TYPE_LONG_DOUBLE => {
            return None;
        }
        ROS_TYPE_CHAR => AttrVal::String(Cow::Owned(
            c_char::from_ne_bytes(slice::from_raw_parts(ptr, 1).try_into().ok()?).to_string(),
        )),
        ROS_TYPE_WCHAR => {
            let wchar = u16::from_ne_bytes(slice::from_raw_parts(ptr, 2).try_into().ok()?);
            AttrVal::String(Cow::Owned(String::from_utf16_lossy(&[wchar])))
        }
        ROS_TYPE_BOOLEAN => AttrVal::Bool(*ptr != 0),
        ROS_TYPE_OCTET | ROS_TYPE_UINT8 => AttrVal::Integer(u8::from_ne_bytes(
            slice::from_raw_parts(ptr, 1).try_into().ok()?,
        ) as i64),
        /*ROS_INT8*/
        9 => AttrVal::Integer(
            i8::from_ne_bytes(slice::from_raw_parts(ptr, 1).try_into().ok()?) as i64,
        ),
        ROS_TYPE_UINT16 => AttrVal::Integer(u16::from_ne_bytes(
            slice::from_raw_parts(ptr, 2).try_into().ok()?,
        ) as i64),
        ROS_TYPE_INT16 => AttrVal::Integer(i16::from_ne_bytes(
            slice::from_raw_parts(ptr, 2).try_into().ok()?,
        ) as i64),
        ROS_TYPE_UINT32 => AttrVal::Integer(u32::from_ne_bytes(
            slice::from_raw_parts(ptr, 4).try_into().ok()?,
        ) as i64),
        ROS_TYPE_INT32 => AttrVal::Integer(i32::from_ne_bytes(
            slice::from_raw_parts(ptr, 4).try_into().ok()?,
        ) as i64),
        ROS_TYPE_UINT64 => BigInt::new_attr_val(u64::from_ne_bytes(
            slice::from_raw_parts(ptr, 8).try_into().ok()?,
        ) as i128),
        ROS_TYPE_INT64 => AttrVal::Integer(i64::from_ne_bytes(
            slice::from_raw_parts(ptr, 8).try_into().ok()?,
        )),
        ROS_TYPE_MESSAGE => {
            // if we get here, we've taken a wrong turn
            return None;
        }
        _ => {
            // Invalid value
            return None;
        }
    })
}
