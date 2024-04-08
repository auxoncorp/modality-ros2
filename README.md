# modality-ros2
Observability ([Modality](https://docs.auxon.io/modality/)) and Mutation ([Deviant](https://docs.auxon.io/deviant/)) support for [ROS 2](https://www.ros.org/).

Supports the following ROS versions:
* humble

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
  wget -O ros_deps https://github.com/auxoncorp/modality-ros2/releases/latest/download/ros_deps_22.04
  ```

3. Set `LD_PRELOAD` environment variable prior to running ROS nodes.
  ```bash
  export LD_PRELOAD=/path/to/libmodality_ros_hook.so:/opt/ros/humble/lib/librmw_fastrtps_cpp.so
  ```

  NB: You must be *100% sure* that the .so library you're using was
  built against *exactly* the same version of ROS that you're
  running. This instrumentation relies on the layout of some internal
  structs, which may change between versions. We build against the
  `ros:humble` docker image, but even that is a moving target; check
  the versions in the `ros_deps` file against your system to make sure
  everything lines up.

  Note that we've also appended 'librmw_fastrtps_cpp.so' to LD_PRELOAD
  as well. This is because, ROS uses a middleware-dispatch library
  that `dlopen`s the appropriate middlware library on the fly,
  depending on configuration. Unfortunately, this mechanism is not
  compatible with `LD_PRELOAD`. By including the specific middleware
  implementation in `LD_PRELOAD` here, the dispatch library isn't
  loaded at all, which allows our `LD_PRELOAD` hooks to work.

### Configuration

These environment variables configure the behavior of the ROS LD_PRELOAD library:

* `MODALITY_ROS_CONNECT_TIMEOUT`: How long to wait, in seconds, for a
  connection to `modalityd`, before giving up. During this time,
  messages are queued in memory. Defaults to 20 seconds.

* `MODALITY_ROS_IGNORED_TOPICS`: A comma-separated list of ROS topics
   which should be completely ignored when relaying data to
   Modality. Be sure to include the '/' at the beginning of the
   topic. Defaults to "/parameter_events". If you want to explicitly
   include ALL topics (with no default ignores), set this to the empty
   string `""`.

* `MODALITY_ROS_MAX_ARRAY_LEN`: When transcribing ROS messages into
   Modality format, what is the largest array that should be allowed?
   Any array longer than this will be ignored. This is meant to allow
   things like vectors and coordinates to be passed into Modality,
   while excluding things like camera frames and LIDAR point
   clouds. Defaults to 12.

* `MODALITY_AUTH_TOKEN`: The content of the auth token to use when
   connecting to Modality. If this is not set, the auth token used by
   the Modality CLI is read from `~/.config/modality_cli/.user_auth_token`.

* `MODALITY_HOST`: The hostname where the modality server is running.

This uses the same connection and configuration infrastructure as
reflector plugins; see the [`modality-reflector` Configuration File documentation](https://docs.auxon.io/modality/ingest/modality-reflector-configuration-file.html)
for more information.
