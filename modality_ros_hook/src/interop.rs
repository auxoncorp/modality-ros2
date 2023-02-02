use cstr::cstr;
use std::{
    borrow::Cow,
    ffi::{c_char, c_int, c_void, CStr},
};
use widestring::U16CStr;

#[repr(C)]
#[derive(Debug)]
pub struct RosIdlMessageTypeSupportT {
    /// String identifier for the type_support.
    pub typesupport_identifier: *const c_char,

    /// Pointer to the message type support library
    pub data: *const c_void,

    /// Pointer to the message type support handler function
    pub func: Option<
        fn(*const RosIdlMessageTypeSupportT, *const c_char) -> *const RosIdlMessageTypeSupportT,
    >,
}

// from rosidl_typesupport_introspection_c
pub const ROSIDL_TYPESUPPORT_INTROSPECTION_C_IDENTIFIER: &CStr =
    cstr!(b"rosidl_typesupport_introspection_c");

// from rosidl_typesupport_introspection_cpp
pub const ROSIDL_TYPESUPPORT_INTROSPECTION_CPP_IDENTIFIER: &CStr =
    cstr!(b"rosidl_typesupport_introspection_cpp");

/// Structure used to describe all fields of a single interface type.
#[repr(C)]
pub struct RosIdlTypesupportIntrospectionCMessageMembers {
    /// The namespace in which the interface resides, e.g. "example_messages__msg" for
    /// example_messages/msg
    pub message_namespace_: *const c_char,

    /// The name of the interface, e.g. "Int16"
    pub message_name_: *const c_char,

    /// The number of fields in the interface
    pub member_count_: u32,

    /// The size of the interface structure in memory
    pub size_of_: usize,

    /// A pointer to the array that describes each field of the interface
    pub members: *const RosIdlTypesupportIntrospectionCMessageMember,

    /// The function used to initialise the interface's in-memory representation
    pub init_function: Option<fn(*mut c_void, RosIdlRuntimeCMessageInitialization) -> *mut c_void>,

    /// The function used to clean up the interface's in-memory representation
    pub fini_function: Option<fn(*mut c_void) -> *mut c_void>,
}

impl std::fmt::Debug for RosIdlTypesupportIntrospectionCMessageMembers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe {
            let members: &[RosIdlTypesupportIntrospectionCMessageMember] =
                std::slice::from_raw_parts(self.members, self.member_count_ as usize);

            f.debug_struct("RosIdlTypesupportIntrospectionCMessageMembers")
                .field("message_namespace_", &debug_c_str(self.message_namespace_))
                .field("message_name_", &debug_c_str(self.message_name_))
                .field("member_count_", &self.member_count_)
                .field("size_of_", &self.size_of_)
                .field("*members", &self.members)
                .field("members", &members)
                .field("init_function", &self.init_function)
                .field("fini_function", &self.fini_function)
                .finish()
        }
    }
}

pub unsafe fn debug_c_str(s: *const c_char) -> Cow<'static, str> {
    if s.is_null() {
        "<NULL>".into()
    } else {
        String::from_utf8_lossy(CStr::from_ptr(s).to_bytes())
    }
}

/// Structure used to describe a single field of an interface type.
// NOTE This is for ROS Galactic. It does change in different releases.
#[repr(C)]
pub struct RosIdlTypesupportIntrospectionCMessageMember {
    /// The name of the field.
    pub name: *const c_char,

    /// The type of the field as a value of the field types enum
    /// rosidl_typesupport_introspection_c_field_types.
    /// e.g. rosidl_typesupport_introspection_c__ROS_TYPE_FLOAT
    pub type_id_: RosIdlTypesupportIntrospectionCFieldTypes,

    /// If the field is a string, the upper bound on the length of the string.
    pub string_upper_bound_: usize,

    /// If the type_id_ value is rosidl_typesupport_introspection_c__ROS_TYPE_MESSAGE,
    /// this points to an array describing the fields of the sub-interface.
    pub members_: *const RosIdlMessageTypeSupportT,

    /// True if this field is an array type, false if it is any other type. An
    /// array has the same value for / type_id_.
    pub is_array_: bool,

    /// If is_array_ is true, this contains the number of members in the array.
    pub array_size_: usize,

    /// If is_array_ is true, this specifies if the array has a maximum size. If it is true, the
    /// value in array_size_ is the maximum size.
    pub is_upper_bound_: bool,

    /// The bytes into the interface's in-memory representation that this field can be found at.
    pub offset_: u32,

    /// If the interface has a default value, this points to it.
    pub default_value_: *const c_void,

    /// If is_array_ is true, a pointer to a function that gives the size of one member of the array.
    pub size_function: Option<fn(*const c_void) -> usize>,

    /// If is_array_ is true, a pointer to a function that gives a const pointer to the member of the
    /// array indicated by index.
    pub get_const_function: Option<fn(*const c_void, usize) -> *const c_void>,

    /// If is_array_ is true, a pointer to a function that gives a pointer to the member of the
    /// array indicated by index.
    pub get_function: Option<fn(*mut c_void, usize) -> *mut c_void>,

    /// Pointer to a function that fetches (i.e. copies) an item from
    /// an array or sequence member. It takes a pointer to the member,
    /// an index (which is assumed to be valid), and a pointer to a
    /// pre-allocated value (which is assumed to be of the correct type).
    ///
    /// Available for array and sequence members.
    fetch_function: Option<fn(*const c_void, usize, *mut c_void) -> *mut c_void>,

    /// Pointer to a function that assigns (i.e. copies) a value to an
    /// item in an array or sequence member. It takes a pointer to the
    /// member, an index (which is assumed to be valid), and a pointer
    /// to an initialized value (which is assumed to be of the correct
    /// type).
    ///
    /// Available for array and sequence members.
    assign_function: Option<fn(*mut c_void, usize, *const c_void) -> *mut c_void>,

    /// If is_array_ is true, a pointer to a function that resizes the array.
    pub resize_function: Option<fn(*mut c_void, usize) -> bool>,
}

impl std::fmt::Debug for RosIdlTypesupportIntrospectionCMessageMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe {
            f.debug_struct("RosIdlTypesupportIntrospectionCMessageMember")
                .field("&name", &self.name)
                .field("name", &debug_c_str(self.name))
                .field("type_id_", &self.type_id_)
                .field("string_upper_bound_", &self.string_upper_bound_)
                .field("members_", &self.members_)
                .field("is_array_", &self.is_array_)
                .field("array_size_", &self.array_size_)
                .field("is_upper_bound_", &self.is_upper_bound_)
                .field("offset_", &self.offset_)
                .field("default_value_", &self.default_value_)
                .field("size_function", &self.size_function)
                .field("get_const_function", &self.get_const_function)
                .field("get_function", &self.get_function)
                // .field("fetch_function", &self.fetch_function)
                // .field("assign_function", &self.assig_function)
                .field("resize_function", &self.resize_function)
                .finish()
        }
    }
}

/// Possible types for message fields on a ROS message
/// The equivalent OMG IDL and C types of the different fields can be found
/// at http://design.ros2.org/articles/idl_interface_definition.html#type-mapping
#[repr(u8)]
#[derive(Debug, PartialEq, Copy, Clone)]
#[allow(unused)]
pub enum RosIdlTypesupportIntrospectionCFieldTypes {
    /// Equivalent to float in C types.
    Float = 1,

    /// Equivalent to double in C types.
    Double = 2,

    /// Equivalent to long double in C types.
    LongDouble = 3,

    /// Equivalent to unsigned char in C types.
    Char = 4,

    /// Equivalent to char16_t in C types.
    WChar = 5,

    /// Equivalent to _Bool in C types.
    Boolean = 6,

    /// Equivalent to unsigned char in C types.
    Octet = 7,

    /// Equivalent to uint8_t in C types.
    UInt8 = 8,

    /// Equivalent to int8_t in C types.
    Int8 = 9,

    /// Equivalent to uint16_t in C types.
    UInt16 = 10,

    /// Equivalent to int16_t in C types.
    Int16 = 11,

    /// Equivalent to uint32_t in C types.
    UInt32 = 12,

    /// Equivalent to int32_t in C types.
    Int32 = 13,

    /// Equivalent to uint64_t in C types.
    UInt64 = 14,

    /// Equivalent to int64_t in C types.
    Int64 = 15,

    /// Equivalent to char * in C types.
    String = 16,

    /// Equivalent to char16_t * in C types.
    WString = 17,

    /// An embedded message type.
    Message = 18,
}

impl RosIdlTypesupportIntrospectionCFieldTypes {
    pub fn byte_size(&self) -> usize {
        use RosIdlTypesupportIntrospectionCFieldTypes::*;
        match self {
            Float => 2,
            Double => 4,
            LongDouble => 8,
            Char => 1,
            WChar => 2,
            Boolean => 1,
            Octet => 1,
            UInt8 => 1,
            Int8 => 1,
            UInt16 => 2,
            Int16 => 2,
            UInt32 => 4,
            Int32 => 4,
            UInt64 => 8,
            Int64 => 8,
            String => 0,
            WString => 0,
            Message => 0,
        }
    }
}

// An array of 8-bit characters terminated by a null byte.
#[repr(C)]
pub struct RosIdlRuntimeCString {
    data: *const c_char,
    size: usize,
    capacity: usize,
}

impl RosIdlRuntimeCString {
    pub unsafe fn as_string(&self) -> String {
        String::from_utf8_lossy(CStr::from_ptr(self.data).to_bytes()).to_string()
    }
}

// An array of 16-bit characters terminated by a null byte.
#[repr(C)]
pub struct RosIdlRuntimeWString {
    data: *const u16,
    size: usize,
    capacity: usize,
}

impl RosIdlRuntimeWString {
    pub unsafe fn as_string(&self) -> Option<String> {
        U16CStr::from_ptr_str(self.data).to_string().ok()
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct RosSequence {
    pub data: *const u8,
    pub size: usize,
    pub capacity: usize,
}

#[repr(C)]
#[derive(Debug)]
#[allow(unused)]
pub enum RosIdlRuntimeCMessageInitialization {
    // Initialize all fields of the message, either with the default value
    // (if the field has one), or with an empty value (generally 0 or an
    // empty string).
    All,

    // Skip initialization of all fields of the message.  It is up to the user to
    // ensure that all fields are initialized before use.
    Skip,

    // Initialize all fields of the message to an empty value (generally 0 or an
    // empty string).
    Zero,

    // Initialize all fields of the message that have defaults; all other fields
    // are left untouched.
    DefaultsOnly,
}

// Ported from rosidl_runtime_c
unsafe fn get_message_typesupport_handle(
    handle: *const RosIdlMessageTypeSupportT,
    identifier: *const c_char,
) -> *const RosIdlMessageTypeSupportT {
    assert!(!handle.is_null(), "Null message typesupport handle");
    let func = (*handle)
        .func
        .expect("Null message typesupport handle function");
    func(handle, identifier)
}

// Ported from rmw_cyclonedds
// iiuc, this is like QueryInterface, more or less
pub unsafe fn get_typesupport(
    type_supports: *const RosIdlMessageTypeSupportT,
) -> *const RosIdlMessageTypeSupportT {
    let ts = get_message_typesupport_handle(
        type_supports,
        ROSIDL_TYPESUPPORT_INTROSPECTION_C_IDENTIFIER.as_ptr(),
    );
    if !ts.is_null() {
        return ts;
    }

    get_message_typesupport_handle(
        type_supports,
        ROSIDL_TYPESUPPORT_INTROSPECTION_CPP_IDENTIFIER.as_ptr(),
    )
}

#[repr(C)]
#[derive(Debug)]
pub struct RmwMessageInfo {
    /// This is ns since the unix epoch
    pub source_timestamp: i64,

    /// This is ns since the unix epoch
    pub received_timestamp: i64,

    pub publisher_gid: RmwGid,

    /// Whether this message is from intra_process communication or not
    pub from_intra_process: bool,
}

#[repr(C)]
#[derive(Debug)]
/// ROS graph ID of the topic
pub struct RmwGid {
    /// Name of the rmw implementation
    pub implementation_identifier: *const c_char,

    /// Byte data Gid value
    pub data: [u8; 24],
}

impl Default for RmwGid {
    fn default() -> Self {
        Self {
            implementation_identifier: std::ptr::null(),
            data: [0; 24],
        }
    }
}

extern "C" {
    pub fn rmw_get_gid_for_publisher(
        publisher: crate::hooks::PublisherPtr,
        gid: *mut RmwGid,
    ) -> c_int;
}
