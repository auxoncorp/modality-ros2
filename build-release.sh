set -e

(
    cd modality-ros-hook
    if [ "$TARGETPLATFORM" = "linux/amd64" ]; then
        cargo clippy --all-features -- -W clippy::all -D warnings
        cargo fmt --all -- --check
        cargo test --all-features -- --test-threads=1
    fi
    cargo build --profile=release-with-debug
)

mkdir -p target/release

gcc -Wall -Wextra -Wbad-function-cast -Wmissing-declarations -Wmissing-include-dirs -Wmissing-prototypes -Wshadow \
    -fPIC -shared \
    -Os \
    -o target/release/libmodality_ros_hook.so \
    ld_preload_shim.c \
    ${BINDGEN_EXTRA_CLANG_ARGS} \
    -L modality-ros-hook/target/release-with-debug/ \
    -ldl -lmodality_ros_hook_rust

record_package_version () {
    local package=$1
    echo "${package}" >> target/release/ros_deps
    dpkg -s "${package}" | grep '^Version:' >> target/release/ros_deps
    echo >> target/release/ros_deps
}

lsb_release -a >> target/release/ros_deps
echo >> target/release/ros_deps

uname -a >> target/release/ros_deps
echo >> target/release/ros_deps

record_package_version "libc-bin"
record_package_version "libc6-dev"
record_package_version "ros-humble-rcutils"
record_package_version "ros-humble-rcpputils"
record_package_version "ros-humble-rosidl-runtime-c"
record_package_version "ros-humble-rosidl-typesupport-interface"
record_package_version "ros-humble-rosidl-typesupport-introspection-c"
record_package_version "ros-humble-rmw"
record_package_version "ros-humble-fastrtps"
record_package_version "ros-humble-rmw-fastrtps-shared-cpp"
record_package_version "ros-humble-fastcdr"
