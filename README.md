# modality-ros2

Observability ([Modality](https://docs.auxon.io/modality/)) and Mutation ([Deviant](https://docs.auxon.io/deviant/)) support for [ROS 2](https://www.ros.org/).

## modality-ros-hook

An `LD_PRELOAD` integration for the rmw (ROS middleware abstraction) interface layer that provides topic
publisher and subscriber tracing.

### Getting Started

1. Get the plugin shared library
  * Build from source
  ```bash
  cargo build --release
  target/release/libmodality_ros_hook.so
  ```
  * Download the latest artifact from github releases
  ```bash
  wget -O libmodality_ros_hook.so https://github.com/auxoncorp/modality-ros2/releases/latest/download/libmodality_ros_hook_22.04_amd64.so
  ```
2. Set `LD_PRELOAD` environment variable prior to running ROS nodes
  ```bash
  export LD_PRELOAD=/path/to/libmodality_ros_hook.so:/opt/ros/humble/lib/librmw_fastrtps_cpp.so
  ```

### Configuration

See the [`modality-reflector` Configuration File documentation](https://docs.auxon.io/modality/ingest/modality-reflector-configuration-file.html) for more information
about the reflector or reflector-plugin configuration.
