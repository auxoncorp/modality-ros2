use core::slice;
use std::ffi::{c_char, c_void, CStr};

use interop::{
    RosIdlRuntimeCString, RosIdlRuntimeWString, RosIdlTypesupportIntrospectionCFieldTypes,
    RosSequence,
};
use modality_api::{AttrVal, BigInt};

mod hooks;
mod interop;
mod message_processor;

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct RosMessageSchema {
    namespace: String,
    name: String,
    size: usize,
    members: Vec<RosMessageMemberSchema>,
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
    Scalar {
        key: String,
        type_id: RosIdlTypesupportIntrospectionCFieldTypes,
        offset: usize,
    },
    ScalarArray {
        key: String,
        type_id: RosIdlTypesupportIntrospectionCFieldTypes,
        offset: usize,
        array_size: usize,
        is_upper_bound: bool,
        size_function: Option<fn(*const c_void) -> usize>,
        get_function: Option<fn(*const c_void, usize) -> *const c_void>,
    },
    MessageSequence {
        key: String,
        offset: usize,
        message_schema: FlatRosMessageSchema,
        array_size: usize,
        is_upper_bound: bool,
        size_function: Option<fn(*const c_void) -> usize>,
        get_function: Option<fn(*const c_void, usize) -> *const c_void>,
    },
}

impl RosMessageSchema {
    #[allow(clippy::if_same_then_else)]
    unsafe fn from_c(type_support: *const interop::RosIdlMessageTypeSupportT) -> Option<Self> {
        let ts = interop::get_typesupport(type_support);
        if ts.is_null() {
            return None;
        }

        let ts_id = CStr::from_ptr((*ts).typesupport_identifier);
        let c_schema: *const interop::RosIdlTypesupportIntrospectionCMessageMembers =
            if ts_id == interop::ROSIDL_TYPESUPPORT_INTROSPECTION_C_IDENTIFIER {
                std::mem::transmute((*ts).data)
            } else if ts_id == interop::ROSIDL_TYPESUPPORT_INTROSPECTION_CPP_IDENTIFIER {
                // they're actually the same exact memory layout
                std::mem::transmute((*ts).data)
            } else {
                unimplemented!(
                    "unknown typesupport type: {}",
                    interop::debug_c_str((*ts).typesupport_identifier)
                )
            };

        let c_members: &[interop::RosIdlTypesupportIntrospectionCMessageMember] =
            std::slice::from_raw_parts((*c_schema).members, (*c_schema).member_count_ as usize);
        let mut members = vec![];
        for c_member in c_members {
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
                target.push(FlatRosMessageMemberSchema::Scalar {
                    key: name_segs.join("."),
                    type_id,
                    offset: initial_offset + offset,
                })
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
                    target.push(FlatRosMessageMemberSchema::ScalarArray {
                        key: name_segs.join("."),
                        type_id,
                        offset,
                        array_size,
                        is_upper_bound,
                        size_function,
                        get_function,
                    })
                }
                RosMessageMemberSchema::NestedMessage {
                    name,
                    offset: _,
                    schema,
                } => {
                    let mut name_segs = prefix.clone();
                    name_segs.push(name);

                    let message_schema = schema.flatten(prefix);

                    target.push(FlatRosMessageMemberSchema::MessageSequence {
                        key: name_segs.join("."),
                        offset,
                        message_schema,
                        array_size,
                        is_upper_bound,
                        size_function,
                        get_function,
                    })
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
    fn interpret_message(
        &self,
        prefix: Option<&str>,
        message: &[u8],
        kvs: &mut Vec<(String, AttrVal)>,
    ) {
        for member in self.members.iter() {
            let _ = member.interpret_message(prefix, message, kvs);
        }
    }
}

impl FlatRosMessageMemberSchema {
    fn interpret_message(
        &self,
        prefix: Option<&str>,
        message: &[u8],
        kvs: &mut Vec<(String, AttrVal)>,
    ) -> Option<()> {
        match self {
            FlatRosMessageMemberSchema::Scalar {
                key,
                type_id,
                offset,
            } => {
                let val = unsafe { ros_to_attr_val(type_id, message[*offset..].as_ptr())? };

                let key = if let Some(p) = prefix {
                    format!("{p}.{key}")
                } else {
                    key.clone()
                };

                //eprintln!("* {key}: {val}");
                kvs.push((key, val));
            }

            FlatRosMessageMemberSchema::ScalarArray {
                key,
                array_size,
                is_upper_bound: _,
                size_function: _,
                get_function,
                type_id,
                offset,
            } => unsafe {
                // let scalar_array_ptr = usize::from_ne_bytes(slice[0..8].try_into().ok()?) as *const c_void;
                let slice = &message[*offset..];
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
                        let item_key = format!("{key}.{i}");
                        let item_ptr = get_function(scalar_array_ptr, i);
                        let val = ros_to_attr_val(type_id, item_ptr as *const u8)?;
                        kvs.push((item_key, val));
                    }
                }
            },

            FlatRosMessageMemberSchema::MessageSequence {
                key,
                offset,
                message_schema,
                array_size: _,
                is_upper_bound: _,
                size_function,
                get_function,
            } => unsafe {
                let slice = &message[*offset..];

                let seq: *const RosSequence = std::mem::transmute(slice.as_ptr());
                let msg_array_len = ((*size_function)?)(seq as _);

                let get_function = (*get_function)?;

                for i in 0..msg_array_len {
                    let item_key = format!("{key}.{i}");
                    for item_field_schema in message_schema.members.iter() {
                        let item_ptr = get_function(slice.as_ptr() as _, i);
                        let item_slice: &[u8] =
                            slice::from_raw_parts(item_ptr as _, message_schema.size);
                        item_field_schema.interpret_message(Some(&item_key), item_slice, kvs);
                    }
                }
            },
        }

        Some(())
    }
}

unsafe fn ros_to_attr_val(
    type_id: &RosIdlTypesupportIntrospectionCFieldTypes,
    ptr: *const u8,
) -> Option<AttrVal> {
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
        | RosIdlTypesupportIntrospectionCFieldTypes::UInt8 => AttrVal::Integer(u8::from_ne_bytes(
            slice::from_raw_parts(ptr, 1).try_into().ok()?,
        ) as i64),
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
