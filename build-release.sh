set -e

(
    cd modality-ros-hook
    cargo build --profile=release-with-debug

)

mkdir -p target/release

gcc -Wall -fPIC -shared \
    -Os \
    -o target/release/libmodality_ros_hook.so \
    ld_preload_shim.c \
    -L modality-ros-hook/target/release-with-debug/ \
    -ldl -lmodality_ros_hook_rust