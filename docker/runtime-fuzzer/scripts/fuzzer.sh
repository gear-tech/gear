#!/bin/bash
set -e

VOLUME_DIR='/home/ubuntu/fuzzing-seeds-dir/'
ARCHIVE_PATH="/opt/download-archives/"
CONTAINER_NAME=node-fuzzer
IMAGE='ghcr.io/gear-tech/gear-node-fuzzer:latest'
DOCKER_EXIT_CODE=''

# Function to check if error was OOM
function _check_need_arch {
    cmd=$(tail -n 50 node-fuzzer.log | grep 'ERROR: libFuzzer: out-of-memory') 
    if [[ $cmd ]]; then
        echo "Archiving doesn't needed due to OOM error"
        return 0
    else
        echo "Proceed with archiving"
        return 1
    fi
}

# Function to check runtime of container in second
function _check_container_runtime {
    START=$(docker inspect --format='{{.State.StartedAt}}' ${CONTAINER_NAME})
    STOP=$(docker inspect --format='{{.State.FinishedAt}}' ${CONTAINER_NAME})
    START_TIMESTAMP=$(date --date=$START +%s)
    STOP_TIMESTAMP=$(date --date=$STOP +%s) 
    echo "Container worked for: $(($STOP_TIMESTAMP-$START_TIMESTAMP)) seconds"
}

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
    _dcode=$(docker inspect ${CONTAINER_NAME} --format='{{.State.ExitCode}}' )
    echo "Container exit code: $_dcode"
    if [ "$_dcode" != 137 ]; then
        # Get the logs from the container and archive them with the current timestamp in the filename
        docker logs node-fuzzer >& node-fuzzer.log
        echo "Copy fuzzing-seeds"
        cp ${VOLUME_DIR}fuzzing-seeds ./

        if [ _check_need_arch ]; then
            echo "Creating tar archive: ${ARCHIVE_NAME}"
            # Tar logs and seeds
            tar -czvf ${ARCHIVE_PATH}/${ARCHIVE_NAME} node-fuzzer.log fuzzing-seeds
        fi
        echo "Clean tmp files"
        rm node-fuzzer.log fuzzing-seeds
    else
        echo "Container was killed manually"
    fi
}

function start {
    # Loop to keep restarting the container if it stops due to an error
    while true; do
        echo "########## $(date) ###########" 
        echo "Start container: ${CONTAINER_NAME}"
        start_container
        echo "Start archiving logs"
        archive_logs
	_check_container_runtime
        # Clean up the container
        docker rm ${CONTAINER_NAME}
	docker rmi ${IMAGE}
	# Clean archives older than 30 days
	find ${ARCHIVE_PATH} -name "node-fuzzer_logs*.tar.gz" -type f -mtime +30 -delete 
    done
}

function stop {
    # Stop the container
    docker stop ${CONTAINER_NAME}
    # Clean up the container
    docker rm ${CONTAINER_NAME}
    docker rmi ${IMAGE}
}

case "$1" in 
    start)   start;;
    stop)    stop;;
    *) echo "usage: $0 start_app|stop_app" >&2
       exit 1
       ;;
esac
