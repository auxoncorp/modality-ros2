FROM modality-ros2-build

FROM ros:humble

RUN apt update
RUN apt upgrade -y
RUN apt install less vim ros-humble-demo-nodes-py psmisc -y
COPY --from=0 /src/target/release/libmodality_ros_hook.so /
COPY run.sh /

# RUN apt install xterm -y
# RUN apt install ros-humble-ros-ign-bridge ros-humble-robot-localization "ros-humble-nav2-*" -y
# RUN apt install ignition-fortress -y
# RUN apt install ros-humble-xacro ros-humble-ros-gz-sim ros-humble-rviz2 -y
#  
# COPY run.sh /
# COPY nav2_outdoor_example /nav2_outdoor_example
# WORKDIR /nav2_outdoor_example
# RUN bash -c "source /opt/ros/humble/setup.bash && colcon build"
# WORKDIR /
