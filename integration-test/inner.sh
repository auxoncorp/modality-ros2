#!/usr/bin/env bash 

source /opt/ros/humble/setup.bash
export LD_PRELOAD=/libmodality_ros_hook.so:/opt/ros/humble/lib/librmw_fastrtps_cpp.so

set -ex

(
    cd /messages
    for msg_file in $(find . -name "*.json"); do
        schema=$(dirname ${msg_file})/$(basename ${msg_file} .json)
        schema=$(echo ${schema} | sed "s/\\.\\///" )
        topic=${schema}
        ros2 topic pub -t 1 -w 0 ${topic} ${schema} "$(cat ${msg_file})"
    done
)


/opt/ros/humble/lib/demo_nodes_py/listener &
sleep 1
/opt/ros/humble/lib/demo_nodes_py/talker &
sleep 5
kill %2
kill %1

/opt/ros/humble/lib/demo_nodes_py/add_two_ints_server &

sleep 1
/opt/ros/humble/lib/demo_nodes_py/add_two_ints_client 
sleep 1
/opt/ros/humble/lib/demo_nodes_py/add_two_ints_client 
sleep 1
/opt/ros/humble/lib/demo_nodes_py/add_two_ints_client 
sleep 1
/opt/ros/humble/lib/demo_nodes_py/add_two_ints_client 
sleep 1

kill %1

