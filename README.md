# modality-ros2
Observability ([Modality](https://docs.auxon.io/modality/)) and Mutation ([Deviant](https://docs.auxon.io/deviant/)) support for [ROS 2](https://www.ros.org/).

Supports the following ROS versions:
* humble

## modality-ros-hook

An `LD_PRELOAD` integration for the rmw (ROS middleware abstraction) interface layer that provides
- Precise topic publisher and subscriber tracing, including exact receive points.
- Precise service (RPC) tracing, on the client and server

## Getting Started

1. Get the plugin shared library
  * Build from source
  ```bash
  cargo build --release
  target/release/libmodality_ros_hook.so
  ```

  * Download the latest artifact from github releases
  ```bash
  wget libmodality_ros_hook.so https://github.com/auxoncorp/modality-ros2/releases/latest/download/libmodality_ros_hook_22.04_amd64.so
  wget ros_deps https://github.com/auxoncorp/modality-ros2/releases/latest/download/ros_deps_22.04_amd64
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

## Configuration

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

* `MODALITY_ROS_BUFFER_SIZE`: How many messages to buffer internally,
  before sending to the Modality backend. If this is too small, some
  message may be dropped (an error message will be printed). Defaults
  to 1024.

* `MODALITY_RUN_ID`: The run id to value to use in timeline metadata
  (`timeline.run_id`). This is used as the basis for the segmentation
  method used in the default Modality workspace.  Defaults to a
  randomly generated uuid.

* `MODALITY_AUTH_TOKEN`: The content of the auth token to use when
  connecting to Modality. If this is not set, the auth token used by
  the Modality CLI is read from
  `~/.config/modality_cli/.user_auth_token`.

* `MODALITY_HOST`: The hostname where the modality server is running.

This library uses the same connection and configuration infrastructure as
reflector plugins; see the [`modality-reflector` Configuration File documentation](https://docs.auxon.io/modality/ingest/modality-reflector-configuration-file.html)
for more information.

## Adapter Concept Mapping

The following describes the default mapping between ROS concepts and
Modality's concepts.

* A timeline is created for each ROS node. The timeline's name is
  created by combining the node's namespace and name with a `/`
  character. Any preceiding `/` in the node's namespace is
  removed. For example, a node with the namespace `/nav` and the name
  `gps` will create a timeline named `nav/gps`.

* When a message is published by a ROS node, an event is created on that node's timeline.

  * The event name is the name of the topic on which the message was
    published, with any preceding '/' character removed. For example,
    a message published on the '/orientation' topic will be logged
    in modality as an event named 'orientation'.

  * The payload data on the message is converted to event attributes.
    * The attribute name is the name of the payload field. For
      example, the `depth` payload field would be converted to the
      `depth` attribute.

    * Nested structures are flattened, with their names connected by
      the `.` character. For example, if a payload contains a
      `velocity` structure, which in turn contain `x` and `y` field,
      the event will contain the attributes `velocity.x` and
      `velocity.y`.

    * Arrays are flattened as well; the array index is used as a
      numeric value, again separated by `.` charcters.  For example,
      if a playload contains an array named `position` with three
      elements, the event will contain the attributes `position.0`,
      `position.1`, and `position.2`.

      * The probe intentionally limits the size of arrays which are
        converted. By default, arrays up to length 12 will be
        converted; anything longer will be ignored. This threshold
        may be customized using the `MODALITY_ROS_MAX_ARRAY_LEN`
        environment variable.

    * Payload values are converted to attribute values directly:
      * String, WString, Char, and WChar are converted to Modality strings

      * Floats and Doubles are converted to Modality floats (64 bit)

      * Booleans are converted to Modality booleans

      * All integer numeric types are converted to Modality integers
        (stored as signed 64 bit). Large UInt64 values may be promoted
        to a Modality BigInt (128 bit).

  * The '/rosout' topic is special cased: the textual content of the
    message is used as the event name.

* When a service interaction is made by a ROS node, an event is created on that node's timeline.

  * One event is created for each step in the service RPC lifecycle:
    send request, take (receive) request, send response, and take
    (receive) response. The request events are logged on the client
    node's timeline, while the response events are logged on the
    servce node's timeline.

  * These events are named using the name of the service. Any
    preceding slashes are removed, and any intermediate slashes are
    replaced with `.` characters. For example, calls to the service
    `/waypoint/create` would be logge as events named
    `waypoint.create`.  The exact service name is also available as
    the `event.ros.service.name` attribute.

  * The different events in the service lifecycle may be distinguished
    using the `event.ros.service.event` attribute.

* The following timeline attributes are added:
  * `timeline.ros.node.namespace`: (string) The exact namespace of the ROS node.

  * `timeline.ros.node.name`: (string) The exact name of the ROS node.

  * `timeline.ros.node`: (string) The exact namespace and name of the ROS node, combined with a `/` character.

  * `timeline.ros.publisher_gid.<gid>`: (boolean) This attribute is set to true for
    a node that publishes messages using the graph id `<gid>`. It is used
    for connecting pubsub-related interactions.

  * `timeline.ros.service.client_guid.<guid>`: (boolean) This attribute is set to
    true for a node which a service client that is identified by `<guid>`. It is used
    for connecting service-related interactions.

* The following attributes are added to pubsub-related events:
  * `event.ros.topic`: (string) The exact name of the topic on which
    message was published (or received).

  * `event.ros.publish`: (bool) This is present, and set to true, for
    events logged when a message is published on a topic.

  * `event.ros.received`: (bool) This is present, and set to true, for
    events logged when a message received on a node that subscribed to a topic.

  * `event.ros.schema.namespace`: (string) The exact namespace of the
    schema for the pubsub message's payload.

  * `event.ros.schema.name`: (string) The exact namespace of the
    schema for the pubsub message's payload.

* The following event attributes are added to service-related events:
  * `event.ros.service.name`: (string) For service-related events,
    this is set to the exact name of the service involved.

  * `event.ros.service.event`: (string) For service-related events,
    this indicates what part of the service transaction is being
    represented. The value is one of `send_request`, `recv_request`,
    `send_response`, or `recv_response`.

* The following additional special events are logged, representing
  operations that occur at the RMW (ROS Middlware) layer:
  * `rmw.create_node` / `rmw.destroy_node`
  * `rmw.create_publisher` / `rmw.destroy_publisher`
  * `rmw.create_subscription` / `rmw.destroy_subscription`
  * `rmw.create_service` / `rmw.destroy_service`
  * `rmw.create_client` / `rmw.destroy_client`
