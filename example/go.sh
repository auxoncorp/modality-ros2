set -e

(
    cd ..
    ./build-in-docker.sh
)

if ! docker image exists ros:humble; then
    docker pull docker.io/ros:humble
fi
docker build . -t modality-ros2-example
docker run -it --rm \
       --net=host \
       -v ~/.config/modality_cli:/root/.config/modality_cli \
       modality-ros2-example  \
       /run.sh
