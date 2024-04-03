use std::env;
use std::path::PathBuf;

fn main() {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    let rmw_bindings = bindgen::Builder::default()
        .header("wrapper.h")
        // Set up include paths for the dev tree. These are not present on
        // the real build. Instead, it installs the headers via the
        // offical deb packages, and sets the include path with
        // BINDGEN_EXTRA_CLANG_ARGS.
        .clang_args([
            "-I/usr/include/rcutils",
            "-I../ros-deps-for-development/rmw/rmw/include",
            "-I../ros-deps-for-development/rosidl/rosidl_runtime_c/include",
            "-I../ros-deps-for-development/rosidl/rosidl_typesupport_interface/include",
            "-I../ros-deps-for-development/rosidl/rosidl_typesupport_introspection_c/include",
        ])
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    rmw_bindings
        .write_to_file(out_path.join("rmw_bindings.rs"))
        .expect("Couldn't write bindings!");

    let rtps_bindings = bindgen::Builder::default()
        .header("wrapper.hpp")
        .clang_args([
            "-I/usr/include/rcutils",
            "-I/usr/include/rcpputils",
            "-I../ros-deps-for-development/rmw/rmw/include",
            "-I../ros-deps-for-development/rosidl/rosidl_runtime_c/include",
            "-I../ros-deps-for-development/rosidl/rosidl_typesupport_interface/include",
            "-I../ros-deps-for-development/rmw_fastrtps/rmw_fastrtps_shared_cpp/include",
        ])
        .allowlist_type("CustomClientInfo")
        .allowlist_type("CustomServiceInfo")
        .opaque_type("std::.*")
        .opaque_type("eprosima::fastdds::dds::.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    rtps_bindings
        .write_to_file(out_path.join("rtps_bindings.rs"))
        .expect("Couldn't write bindings!");
}
