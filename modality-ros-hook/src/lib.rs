use core::slice;
use std::ffi::{c_char, c_void, CStr};

use interop::{
    RosIdlRuntimeCString, RosIdlRuntimeWString, RosIdlTypesupportIntrospectionCFieldTypes,
    RosSequence,
};
use itertools::Itertools;
use modality_api::{AttrVal, BigInt};

mod hooks;
mod interop;
mod message_processor;

#[derive(Debug, Clone)]
#[allow(unused)]
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
        type_id: RosIdlTypesupportIntrospectionCFieldTypes,
        offset: usize,
    },
    Array {
        offset: usize,
        item_schema: Box<RosMessageMemberSchema>,
        array_size: usize,
        is_upper_bound: bool,
        size_function: Option<fn(*const c_void) -> usize>,
        get_function: Option<fn(*const c_void, usize) -> *const c_void>,
    },
    NestedMessage {
        name: String,
        offset: usize,
        schema: RosMessageSchema,
    },
}

#[derive(Debug, Clone)]
#[allow(unused)]
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
    pub type_id: RosIdlTypesupportIntrospectionCFieldTypes,
    pub offset: usize,
}

#[derive(Debug, Clone)]
pub struct ScalarArrayMemberSchema {
    pub key: String,
    pub type_id: RosIdlTypesupportIntrospectionCFieldTypes,
    pub offset: usize,
    pub array_size: usize,
    pub is_upper_bound: bool,
    pub size_function: Option<fn(*const c_void) -> usize>,
    pub get_function: Option<fn(*const c_void, usize) -> *const c_void>,
}

#[derive(Debug, Clone)]
pub struct MessageSequenceMemberSchema {
    pub key: String,
    pub offset: usize,
    pub message_schema: FlatRosMessageSchema,
    pub array_size: usize,
    pub is_upper_bound: bool,
    pub size_function: Option<fn(*const c_void) -> usize>,
    pub get_function: Option<fn(*const c_void, usize) -> *const c_void>,
}

impl RosMessageSchema {
    #[allow(clippy::if_same_then_else)]
    unsafe fn from_c(type_support: *const interop::RosIdlMessageTypeSupportT) -> Option<Self> {
        let ts = interop::get_typesupport(type_support);
        if ts.is_null() {
            return None;
        }

        //println!("frobulate");
        if (*ts).typesupport_identifier.is_null() {
            return None;
        }
        let ts_id = CStr::from_ptr((*ts).typesupport_identifier);
        let c_schema: *const interop::RosIdlTypesupportIntrospectionCMessageMembers =
            if ts_id == interop::ROSIDL_TYPESUPPORT_INTROSPECTION_C_IDENTIFIER {
                std::mem::transmute((*ts).data)
            } else if ts_id == interop::ROSIDL_TYPESUPPORT_INTROSPECTION_CPP_IDENTIFIER {
                // they're actually the same exact memory layout
                std::mem::transmute((*ts).data)
                //return None;
            } else {
                unimplemented!(
                    "unknown typesupport type: {}",
                    interop::debug_c_str((*ts).typesupport_identifier)
                )
            };

        if (*c_schema).members.is_null() {
            return None;
        }

        let c_members: &[interop::RosIdlTypesupportIntrospectionCMessageMember] =
            std::slice::from_raw_parts((*c_schema).members, (*c_schema).member_count_ as usize);
        let mut members = vec![];
        for c_member in c_members {
            if c_member.name.is_null() {
                continue;
            }

            let name =
                String::from_utf8_lossy(CStr::from_ptr(c_member.name).to_bytes()).to_string();
            let type_id = c_member.type_id_;
            let offset = c_member.offset_ as usize;

            let member = if c_member.is_array_ {
                let item_schema = if type_id == RosIdlTypesupportIntrospectionCFieldTypes::Message {
                    Box::new(RosMessageMemberSchema::NestedMessage {
                        name,
                        offset,
                        schema: RosMessageSchema::from_c(c_member.members_).unwrap(),
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
            } else if type_id == RosIdlTypesupportIntrospectionCFieldTypes::Message {
                RosMessageMemberSchema::NestedMessage {
                    name,
                    offset,
                    schema: RosMessageSchema::from_c(c_member.members_).unwrap(),
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
        let mut members = vec![];
        for member in self.members.into_iter() {
            member.flatten(prefix, 0, &mut members);
        }

        FlatRosMessageSchema {
            namespace: self.namespace,
            name: self.name,
            size: self.size,
            members,
        }
    }
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
        // let name = self.name().to_string();
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
                offset,
            } => match *item_schema {
                RosMessageMemberSchema::Scalar {
                    name,
                    type_id,
                    offset: _,
                } => {
                    let mut name_segs = prefix.clone();
                    name_segs.push(name);
                    target.push(FlatRosMessageMemberSchema::ScalarArray(
                        ScalarArrayMemberSchema {
                            key: name_segs.join("."),
                            type_id,
                            offset,
                            array_size,
                            is_upper_bound,
                            size_function,
                            get_function,
                        },
                    ))
                }
                RosMessageMemberSchema::NestedMessage {
                    name,
                    offset: _,
                    schema,
                } => {
                    let mut name_segs = prefix.clone();
                    name_segs.push(name);

                    let message_schema = schema.flatten(prefix);

                    target.push(FlatRosMessageMemberSchema::MessageSequence(
                        MessageSequenceMemberSchema {
                            key: name_segs.join("."),
                            offset,
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
    fn interpret_message<'a>(
        &'a self,
        message: &'a [u8],
    ) -> impl Iterator<Item = Vec<(String, AttrVal)>> + 'a {
        if (self.namespace == "diagnostic_msgs::msg" || self.namespace == "diagnostic_msgs__msg") && self.name == "DiagnosticArray" {
            itertools::Either::Left(self.interepret_as_diagnostic_array(message))
        } else {
            let mut kvs = vec![
                (
                    "event.ros.schema.namespace".to_string(),
                    AttrVal::String(self.namespace.clone()),
                ),
                (
                    "event.ros.schema.name".to_string(),
                    AttrVal::String(self.name.clone()),
                ),
            ];

            self.interpret_message_internal(None, message, &mut kvs);

            if let Some(msg) = extract_log_message(self, &kvs) {
                kvs.push(("name".to_string(), msg));
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
                AttrVal::String(self.namespace.clone()),
            ),
            (
                "event.ros.schema.name".to_string(),
                AttrVal::String(self.name.clone()),
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
                                kvs.push((format!("{ks}.key"), AttrVal::String(k.to_string())));
                            } else {
                                // TODO ???
                            }

                            Some(())

                            // status.values.buffer_overruns=0
                            // status.values.buffer_overruns.key = "Buffer overruns"
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
            if !val_slice.as_ptr().is_null() {
                let key = if let Some(p) = prefix {
                    format!("{p}.{key}")
                } else {
                    key.clone()
                };

                //println!("Scalar Member: {key}");

                let val = unsafe { ros_to_attr_val(type_id, val_slice.as_ptr())? };

                kvs.push((key, val));
            }
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
            offset,
        } = &self;

        unsafe {
            if let Some(slice) = message.get(*offset..) {
                let scalar_array_ptr = (slice).as_ptr() as *const c_void;

                let seq: *const RosSequence = std::mem::transmute(slice.as_ptr());

                // This *SHOULD* work. I don't know why it doesn't.
                // let scalar_array_len = ((*size_function)?)(scalar_array_ptr);

                let scalar_array_len = if *array_size != 0 {
                    *array_size
                } else {
                    (*seq).size
                };

                if scalar_array_len > 12 {
                    //eprintln!("skipping large array: {scalar_array_len}");
                    return Some(());
                }

                if !scalar_array_ptr.is_null() {
                    let get_function = (*get_function)?;

                    for i in 0..scalar_array_len {
                        let item_key = if let Some(p) = prefix {
                            format!("{p}.{key}.{i}")
                        } else {
                            format!("{key}.{i}")
                        };

                        //println!("Scalar Array Member: {item_key}");
                        let item_ptr = get_function(scalar_array_ptr, i);
                        if !item_ptr.is_null() {
                            let val = ros_to_attr_val(type_id, item_ptr as *const u8)?;
                            kvs.push((item_key, val));
                        }
                    }
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
                let seq: *const RosSequence = std::mem::transmute(slice.as_ptr());
                let msg_array_len = ((self.size_function)?)(seq as _);
                let get_function = self.get_function?;

                for i in 0..msg_array_len {
                    let item_ptr = get_function(slice.as_ptr() as _, i);
                    let item_slice: &[u8] = if !item_ptr.is_null() {
                        slice::from_raw_parts(item_ptr as _, self.message_schema.size)
                    } else {
                        &[]
                    };
                    f(i, item_slice)?;
                }
            }
            Some(())
        }
    }
}

unsafe fn ros_to_attr_val(
    type_id: &RosIdlTypesupportIntrospectionCFieldTypes,
    ptr: *const u8,
) -> Option<AttrVal> {
    if ptr.is_null() {
        return None;
    }

    Some(match type_id {
        RosIdlTypesupportIntrospectionCFieldTypes::String => {
            let rt_str: *const RosIdlRuntimeCString = std::mem::transmute(ptr);
            AttrVal::String((*rt_str).as_string())
        }
        RosIdlTypesupportIntrospectionCFieldTypes::WString => {
            let rt_str: *const RosIdlRuntimeWString = std::mem::transmute(ptr);
            AttrVal::String((*rt_str).as_string()?)
        }
        RosIdlTypesupportIntrospectionCFieldTypes::Float => AttrVal::Float(
            (f32::from_ne_bytes(slice::from_raw_parts(ptr, 4).try_into().ok()?) as f64).into(),
        ),
        RosIdlTypesupportIntrospectionCFieldTypes::Double => AttrVal::Float(
            f64::from_ne_bytes(slice::from_raw_parts(ptr, 8).try_into().ok()?).into(),
        ),
        RosIdlTypesupportIntrospectionCFieldTypes::LongDouble => {
            return None;
        }
        RosIdlTypesupportIntrospectionCFieldTypes::Char => AttrVal::String(
            c_char::from_ne_bytes(slice::from_raw_parts(ptr, 1).try_into().ok()?).to_string(),
        ),
        RosIdlTypesupportIntrospectionCFieldTypes::WChar => {
            let wchar = u16::from_ne_bytes(slice::from_raw_parts(ptr, 2).try_into().ok()?);
            AttrVal::String(String::from_utf16_lossy(&[wchar]))
        }
        RosIdlTypesupportIntrospectionCFieldTypes::Boolean => AttrVal::Bool(*ptr != 0),
        RosIdlTypesupportIntrospectionCFieldTypes::Octet
        | RosIdlTypesupportIntrospectionCFieldTypes::UInt8 => {
            let av = AttrVal::Integer(u8::from_ne_bytes(
                slice::from_raw_parts(ptr, 1).try_into().ok()?,
            ) as i64);

            /*if ptr as usize <= 0xFF {
                println!("u8: {}", ptr as i64);
                println!("ptr-u8: {}", *ptr);
                println!("as attrval: {av:?}");
                panic!("definitely not a pointer");
            }*/

            av
        }
        RosIdlTypesupportIntrospectionCFieldTypes::Int8 => AttrVal::Integer(i8::from_ne_bytes(
            slice::from_raw_parts(ptr, 1).try_into().ok()?,
        ) as i64),
        RosIdlTypesupportIntrospectionCFieldTypes::UInt16 => AttrVal::Integer(u16::from_ne_bytes(
            slice::from_raw_parts(ptr, 2).try_into().ok()?,
        ) as i64),
        RosIdlTypesupportIntrospectionCFieldTypes::Int16 => AttrVal::Integer(i16::from_ne_bytes(
            slice::from_raw_parts(ptr, 2).try_into().ok()?,
        ) as i64),
        RosIdlTypesupportIntrospectionCFieldTypes::UInt32 => AttrVal::Integer(u32::from_ne_bytes(
            slice::from_raw_parts(ptr, 4).try_into().ok()?,
        ) as i64),
        RosIdlTypesupportIntrospectionCFieldTypes::Int32 => AttrVal::Integer(i32::from_ne_bytes(
            slice::from_raw_parts(ptr, 4).try_into().ok()?,
        ) as i64),
        RosIdlTypesupportIntrospectionCFieldTypes::UInt64 => BigInt::new_attr_val(
            u64::from_ne_bytes(slice::from_raw_parts(ptr, 8).try_into().ok()?) as i128,
        ),
        RosIdlTypesupportIntrospectionCFieldTypes::Int64 => AttrVal::Integer(i64::from_ne_bytes(
            slice::from_raw_parts(ptr, 8).try_into().ok()?,
        )),
        RosIdlTypesupportIntrospectionCFieldTypes::Message => {
            // if we get here, we've taken a wrong turn
            return None;
        }
    })
}
