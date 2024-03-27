set -e

(
    cd modality-ros-hook
    cargo build
)

mkdir -p target/debug

gcc -Wall -fPIC -shared \
    -Og \
    -o target/debug/libmodality_ros_hook.so \
    ld_preload_shim.c \
    -Irmw/rmw/include \
    -Ircutils/include \
    -Irosidl/rosidl_runtime_c/include \
    -Irosidl/rosidl_typesupport_introspection_c/include \
    -Irosidl/rosidl_typesupport_interface/include \
    -L modality-ros-hook/target/debug/ \
    -ldl -lmodality_ros_hook_rust
