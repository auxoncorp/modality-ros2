#!/usr/bin/env bash
set -ex

(
    cd ..
    ./build-in-docker.sh
)
docker build . -t modality-ros2-integration-test

function cleanup {
  docker stop modalityd || true
  docker logs modalityd || true
  docker rm modalityd || true
}
trap cleanup EXIT

modality_url="http://localhost:14181/v1"
modalityd_image="ghcr.io/auxoncorp/modalityd-nightly:latest"
reflector_image="ghcr.io/auxoncorp/modality-reflector-nightly:latest"

mkdir -p modalityd_data
docker run \
       --name modalityd \
       --network=host \
       -v "$(pwd)/modalityd_data:/data-dir" \
       -e MODALITY_ACCEPT_EULA=Y \
       -e MODALITY_LICENSE_KEY \
       -e NO_TLS=Y \
       -d \
       ${modalityd_image}

curl --retry-max-time 30 --retry 10 --retry-connrefused ${modality_url}/alive

modality_auth_token=$(docker run -t --rm --net=host \
                             -e PATH=/ \
                             --entrypoint /bin/bash \
                             ${reflector_image} \
                             -c "modality user create test > /dev/null && modality user auth-token")
modality_auth_token="$(echo -e "${modality_auth_token}" | tr -d '[:space:]')"

docker run -t --rm \
       --net=host \
       -e MODALITY_AUTH_TOKEN="${modality_auth_token}" \
       -e MODALITY_RUN_ID=1 \
       -v ~/.config/modality_cli:/root/.config/modality_cli \
       modality-ros2-integration-test


docker run -t --rm --net=host \
       -e PATH=/ \
       -e MODALITY_AUTH_TOKEN=${modality_auth_token} \
       --entrypoint /bin/bash \
       -v ./smoke.speqtr:/smoke.speqtr \
       ${reflector_image} \
       -c "conform spec eval --file /smoke.speqtr --dry-run"

