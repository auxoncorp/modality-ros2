FROM ros:humble

RUN apt-get update
RUN apt-get install libssl-dev libclang-dev curl pkg-config -y

RUN curl https://sh.rustup.rs -sSf | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

COPY . /src
WORKDIR /src
ENV BINDGEN_EXTRA_CLANG_ARGS="-I/opt/ros/humble/include/rcutils -I/opt/ros/humble/include/rcpputils -I/opt/ros/humble/include/rosidl_runtime_c -I/opt/ros/humble/include/rosidl_typesupport_interface -I/opt/ros/humble/include/rosidl_typesupport_introspection_c  -I/opt/ros/humble/include/rmw -I/opt/ros/humble/include/rmw_fastrtps_shared_cpp -I/opt/ros/humble/include/fastrtps -I/opt/ros/humble/include/fastcdr"

RUN ./build-release.sh
