#!/bin/bash
set -e

VOLUME_DIR='/home/ubuntu/fuzzing-seeds-dir/'
ARCHIVE_PATH="/opt/download-archives/"
CONTAINER_NAME=node-fuzzer
IMAGE='ghcr.io/gear-tech/gear-node-fuzzer:latest'

# Function to start the container and wait for it to stop
function start_container {
    # Start the container in the background
    if [ ! "$(docker ps -a -q -f name=${CONTAINER_NAME})" ]; then
        if [ "$(docker ps -aq -f status=exited -f name=${CONTAINER_NAME})" ]; then
            # cleanup
            docker rm ${CONTAINER_NAME}
        fi
    # run container
    docker run -d --pull=always \
        -e TERM=xterm-256color \
        -v "${VOLUME_DIR}:/fuzzing-seeds-dir" \
        --name ${CONTAINER_NAME} ${IMAGE}
    fi
    # Wait for the container to stop
    docker wait node-fuzzer
}

function archive_logs {
    ARCHIVE_NAME="node-fuzzer_logs_$(date +%Y-%m-%d_%H-%M-%S).tar.gz"
    # Get the logs from the container and archive them with the current timestamp in the filename
    docker logs node-fuzzer >& node-fuzzer.log
    echo "Copy fuzzing-seeds"
    cp ${VOLUME_DIR}fuzzing-seeds ./
    echo "Creating tar archive: ${ARCHIVE_NAME}"
    # Tar logs and seeds
    tar -czvf ${ARCHIVE_PATH}/${ARCHIVE_NAME} node-fuzzer.log fuzzing-seeds
    echo "Clean tmp files"
    rm node-fuzzer.log fuzzing-seeds
}

#function create_alert {
    # Send a notificatoin to Alertmanager
    #curl -XPOST -d \
    # "{\"alerts\":[{\"status\":\"firing\",\"labels\":{\"alertname\":\"Container Stopped\",\"service\":\"mycontainer\",\"severity\":\"critical\"},\"annotations\":{\"summary\":\"Container nodefuzzer has stopped due to an error. Check the logs for more information.\",\"description\":\"http://ec2-3-101-76-155.us-west-1.compute.amazonaws.com/${filename}\"}}]}" \
    # http://alertmanager.example.com:9093/api/v1/alerts
#}

# Loop to keep restarting the container if it stops due to an error
while true; do
    echo "########## $(date) ###########" 
    echo "Start container: ${CONTAINER_NAME}"
    start_container
    echo "Start archiving logs"
    archive_logs
    # Clean up the container
    docker rm ${CONTAINER_NAME}
done