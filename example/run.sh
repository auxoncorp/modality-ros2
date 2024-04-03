export LD_PRELOAD=/libmodality_ros_hook.so:/opt/ros/humble/lib/librmw_fastrtps_cpp.so

# /opt/ros/humble/lib/demo_nodes_py/listener &
# sleep 1
# /opt/ros/humble/lib/demo_nodes_py/talker &
# sleep 5
# kill %2
# kill %1

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

