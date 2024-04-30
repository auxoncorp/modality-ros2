set -e

for arch in amd64 arm64; do
    platform=linux/${arch}

    # required packages: qemu-user-static binfmt-support
    docker buildx build --platform=${platform} -f Dockerfile.builder -t modality-ros2-build:${arch} .

    mkdir -p target/docker-release/${arch}
    id=$(docker create modality-ros2-build:${arch})
    docker cp $id:/src/target/release/libmodality_ros_hook.so target/docker-release/${arch}
    docker cp $id:/src/target/release/ros_deps target/docker-release/${arch}
    docker rm -v $id
done
