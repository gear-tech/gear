#!/bin/bash
set -e

WORK_DIR=$(pwd -P)
VOLUME_DIR="$WORK_DIR/fuzzing-seeds-dir/"
CORPUS_DIR="$WORK_DIR/corpus/"
ARTIFACT_DIR="$WORK_DIR/artifacts"
ARCHIVE_PATH="/opt/download-archives/"
# DOCKER PARAMS
CONTAINER_NAME=node-fuzzer
IMAGE='ghcr.io/gear-tech/gear-node-fuzzer:latest'
# ALERTING PARAMS
GROUP_ID='***'
BOT_TOKEN='***'

# Function to check container
function _check_need_arch {
    _dcode=$(docker inspect ${CONTAINER_NAME} --format='{{.State.ExitCode}}' )
    echo "Container exit code: $_dcode"
    cmd=$(docker logs -f --tail 100 ${CONTAINER_NAME} | grep 'ERROR: libFuzzer: out-of-memory')
    if [ "$_dcode" -eq 137 ]; then
        echo "Container was stopped manually"
        return 0
    elif [[ $cmd ]]; then
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
function _start_container {
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
        -v "${CORPUS_DIR}:/corpus/main" \
        -v "${ARTIFACT_DIR}:/gear/utils/runtime-fuzzer/fuzz/artifactis/main" \
        --name ${CONTAINER_NAME} ${IMAGE}
    fi
    # Wait for the container to stop
    docker wait node-fuzzer
}

# Send message+logfile to telegram 
function _alert_tg {
    docker logs -f --tail 100 ${CONTAINER_NAME} >& node-fuzzer.log
    text_message="<b>Node Fuzzer Alert</b> 🔥

<b>Problem:</b> Node Fuzzer Container terminated due to an error.
Please check logs.

<b>Archive link:</b>
<a href='http://ec2-3-101-133-141.us-west-1.compute.amazonaws.com/$1'>Link</a>"

    echo "$WORK_DIR"
    curl -s \
        --data "text=$text_message" --data "chat_id=$GROUP_ID" \
        'https://api.telegram.org/bot'$BOT_TOKEN'/sendMessage?parse_mode=HTML'
    curl -v \
        -F "chat_id=$GROUP_ID" \
        -F document=@$WORK_DIR/node-fuzzer.log \
        'https://api.telegram.org/bot'$BOT_TOKEN'/sendDocument'
    rm node-fuzzer.log
}

function _archive_logs {
    ARCHIVE_NAME="node-fuzzer_logs_$(date +%Y-%m-%d_%H-%M-%S).tar.gz"
    echo "Container exit code: $_dcode"
    # Get the logs from the container and archive them with the current timestamp in the filename
    docker logs node-fuzzer >& node-fuzzer.log
    split -C 1024m --additional-suffix=.log --numeric-suffixes node-fuzzer.log node-fuzzer_part
    rm node-fuzzer.log
    echo "Copy fuzzing-seeds"
    cp ${VOLUME_DIR}fuzzing-seeds ./
    echo "Creating tar archive: ${ARCHIVE_NAME}"
    # Tar logs and seeds
    tar -czvf ${ARCHIVE_PATH}/${ARCHIVE_NAME} *.log fuzzing-seeds corpus artifacts
    # Clean temp files
    echo "Clean tmp files"
    rm *.log fuzzing-seeds 
    rm -rf ./atrifacts/*
}

function start {
    # Loop to keep restarting the container if it stops due to an error
    while true; do
        echo "########## $(date) ###########" 
        echo "Start container: ${CONTAINER_NAME}"
        _start_container
        if ! _check_need_arch; then
            echo "Start archiving logs"
            _archive_logs
            echo "Create alert"
            _alert_tg $ARCHIVE_NAME
        fi
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
