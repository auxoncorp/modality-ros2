/opt/ros/humble/lib/demo_nodes_py/listener &
sleep 1
/opt/ros/humble/lib/demo_nodes_py/talker &
sleep 10
killall talker
killall listener
