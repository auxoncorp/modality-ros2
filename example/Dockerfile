FROM modality-ros2-build

FROM ros:humble
RUN apt update
RUN apt upgrade -y
RUN apt install less vim ros-humble-demo-nodes-py psmisc -y
COPY --from=0 /src/target/release/libmodality_ros_hook.so /
COPY run.sh /
