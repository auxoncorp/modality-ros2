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
    -L modality-ros-hook/target/debug/ \
    -ldl -lmodality_ros_hook_rust
