set -e

docker build -f Dockerfile.builder -t modality-ros2-build .

mkdir -p target/docker-release/
id=$(docker create modality-ros2-build)
docker cp $id:/src/target/release/libmodality_ros_hook.so target/docker-release/
docker cp $id:/src/target/release/ros_deps target/docker-release/
docker rm -v $id
