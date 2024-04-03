mkdir -p ros-deps-for-development

(
    cd ros-deps-for-development
    git clone git@github.com:ros2/rmw.git --branch humble
    git clone git@github.com:ros2/rosidl.git --branch humble
    git clone git@github.com:ros2/rmw_fastrtps.git --branch humble
)

echo "Cloned! You'll also want to install the following packges (tested in debian bookworm):"
echo "  apt install librcutils-dev librcpputils-dev libfastcdr-dev"
