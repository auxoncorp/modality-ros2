#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused)]

use std::{
    borrow::Cow,
    ffi::{c_char, CStr},
};
use widestring::U16CStr;

#[allow(improper_ctypes)]
#[allow(clippy::all)]
mod raw {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/rmw_bindings.rs"));

    impl rosidl_runtime_c__String {
        pub unsafe fn as_string(&self) -> String {
            String::from_utf8_lossy(CStr::from_ptr(self.data).to_bytes()).to_string()
        }
    }

    impl rosidl_runtime_c__U16String {
        pub unsafe fn as_string(&self) -> Option<String> {
            U16CStr::from_ptr_str(self.data).to_string().ok()
        }
    }
}

#[allow(improper_ctypes)]
#[allow(clippy::all)]
mod raw_rtps {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/rtps_bindings.rs"));
}

pub unsafe fn debug_c_str(s: *const c_char) -> Cow<'static, str> {
    if s.is_null() {
        "<NULL>".into()
    } else {
        String::from_utf8_lossy(CStr::from_ptr(s).to_bytes())
    }
}

pub mod rosidl {
    pub use super::raw::rosidl_message_type_support_t as message_type_support;
    pub use super::raw::rosidl_service_type_support_t as service_type_support;
    use super::*;

    pub mod runtime {
        use super::*;

        pub use raw::get_message_typesupport_handle;
        pub use raw::get_service_typesupport_handle;
        pub use raw::rosidl_runtime_c__String as String;
        pub use raw::rosidl_runtime_c__U16String as U16String;
        pub use raw::rosidl_runtime_c__byte__Sequence as ByteSequence;
    }

    pub mod typesupport_introspection {
        use super::*;
        pub use raw::rosidl_typesupport_introspection_c__MessageMember as MessageMember;
        pub use raw::rosidl_typesupport_introspection_c__MessageMembers as MessageMembers;
        pub use raw::rosidl_typesupport_introspection_c__ServiceMembers_s as ServiceMembers;
        pub use raw::rosidl_typesupport_introspection_c_field_types as field_type;

        // from rosidl_typesupport_introspection_c
        pub const C_IDENTIFIER: &CStr = c"rosidl_typesupport_introspection_c";

        // from rosidl_typesupport_introspection_cpp
        pub const CPP_IDENTIFIER: &CStr = c"rosidl_typesupport_introspection_cpp";

        pub mod field_types {
            use super::*;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_BOOL as ROS_TYPE_BOOL;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_BOOLEAN as ROS_TYPE_BOOLEAN;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_BYTE as ROS_TYPE_BYTE;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_CHAR as ROS_TYPE_CHAR;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_DOUBLE as ROS_TYPE_DOUBLE;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_FLOAT as ROS_TYPE_FLOAT;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_FLOAT32 as ROS_TYPE_FLOAT32;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_FLOAT64 as ROS_TYPE_FLOAT64;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_INT16 as ROS_TYPE_INT16;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_INT32 as ROS_TYPE_INT32;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_INT64 as ROS_TYPE_INT64;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_INT8 as ROS_TYPE_INT8;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_LONG_DOUBLE as ROS_TYPE_LONG_DOUBLE;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_MESSAGE as ROS_TYPE_MESSAGE;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_OCTET as ROS_TYPE_OCTET;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_STRING as ROS_TYPE_STRING;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_UINT16 as ROS_TYPE_UINT16;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_UINT32 as ROS_TYPE_UINT32;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_UINT64 as ROS_TYPE_UINT64;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_UINT8 as ROS_TYPE_UINT8;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_WCHAR as ROS_TYPE_WCHAR;
            pub use raw::rosidl_typesupport_introspection_c_field_types_rosidl_typesupport_introspection_c__ROS_TYPE_WSTRING as ROS_TYPE_WSTRING;
        }
    }
}

pub mod rmw {
    use super::*;
    pub use raw::rmw_client_t as client;
    pub use raw::rmw_get_gid_for_publisher as get_gid_for_publisher;
    pub use raw::rmw_gid_t as gid;
    pub use raw::rmw_message_info_t as message_info;
    pub use raw::rmw_publisher_t as publisher;
    pub use raw::rmw_request_id_t as request_id;
    pub use raw::rmw_service_info_t as service_info;
    pub use raw::rmw_service_t as service;

    // Ported from rmw_cyclonedds
    // iiuc, this is like QueryInterface, more or less
    pub unsafe fn get_typesupport(
        type_supports: *const rosidl::message_type_support,
    ) -> *const rosidl::message_type_support {
        let ts = rosidl::runtime::get_message_typesupport_handle(
            type_supports,
            rosidl::typesupport_introspection::C_IDENTIFIER.as_ptr(),
        );
        if !ts.is_null() {
            return ts;
        }

        rosidl::runtime::get_message_typesupport_handle(
            type_supports,
            rosidl::typesupport_introspection::CPP_IDENTIFIER.as_ptr(),
        )
    }

    pub unsafe fn get_service_typesupport(
        type_supports: *const rosidl::service_type_support,
    ) -> *const rosidl::service_type_support {
        let ts = rosidl::runtime::get_service_typesupport_handle(
            type_supports,
            rosidl::typesupport_introspection::C_IDENTIFIER.as_ptr(),
        );
        if !ts.is_null() {
            return ts;
        }

        rosidl::runtime::get_service_typesupport_handle(
            type_supports,
            rosidl::typesupport_introspection::CPP_IDENTIFIER.as_ptr(),
        )
    }
}

pub mod fastrtps {
    use super::*;
    pub use raw_rtps::CustomClientInfo;
    pub use raw_rtps::CustomServiceInfo;
}
