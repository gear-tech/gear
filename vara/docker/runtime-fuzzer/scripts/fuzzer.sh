#!/bin/bash
set -e

WORK_DIR=$(pwd -P)
VOLUME_DIR="$WORK_DIR/fuzzing-seeds-dir/"
CORPUS_DIR="$WORK_DIR/corpus/"
ARTIFACT_DIR="$WORK_DIR/artifacts"
ARCHIVE_PATH="/opt/download-archives/"
# DOCKER PARAMS
CONTAINER_NAME=node-fuzzer
CONTAINER_NAME_GEAR=gear
#IMAGE='node-fuzzer:0.0.0'
IMAGE='ghcr.io/gear-tech/gear-node-fuzzer:latest'
DOCKER_EXIT_CODE=''
# ALERTING PARAMS
GROUP_ID='***'
BOT_TOKEN='***'
#HTTP
URL='***'

# Function to check container was stopped manually
function _check_stop_manually {
    _dcode=$(docker inspect ${CONTAINER_NAME} --format='{{.State.ExitCode}}' )
    echo "Container exit code: $_dcode"
    if [ "$_dcode" -eq 137 ]; then
        echo "Container was stopped manually"
        return 0
    else
        echo "Proceed with archiving"
        return 1
    fi
}

# Function to check container was stopped by OOM
function _check_stop_oom {
    cmd=$(docker logs -f --tail 100 ${CONTAINER_NAME} | grep 'ERROR: libFuzzer: out-of-memory') 
    if [ -n $cmd ]; then
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
function start_container_post {
    # Start the container in the background
     if [ "$(docker ps -aq -f status=exited -f name=${CONTAINER_NAME_GEAR})" ]; then
         # cleanup
         docker rm ${CONTAINER_NAME_GEAR}
     fi
     # run container
     docker run --rm -itd  \
     	--entrypoint "/bin/sh" \
     	-e TERM=xterm-256color \
     	-v "${CORPUS_DIR}:/corpus/main" \
     	--workdir /gear/utils/runtime-fuzzer \
     	--name ${CONTAINER_NAME_GEAR} ${IMAGE} \
     	-c "cargo install cargo-binutils && \
		rustup component add llvm-tools && \
		rustup component add --toolchain nightly llvm-tools && \
		cargo fuzz coverage --release --sanitizer=none main /corpus/main -- \
        -rss_limit_mb=8192 -max_len=450000 -len_control=0 && \
		cargo cov -- show target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/main \
        --format=text \
        --show-line-counts \
        --Xdemangler=rustfilt \
        --ignore-filename-regex=/rustc/ \
        --ignore-filename-regex=.cargo/  \
        --instr-profile=fuzz/coverage/main/coverage.profdata > /corpus/main/coverage_$1.txt 2>&1 && \
        cargo cov -- export target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/main \
        --format=lcov \
        --instr-profile=fuzz/coverage/main/coverage.profdata \
        --ignore-filename-regex=/rustc/ \
        --ignore-filename-regex=.cargo/ > /corpus/main/lcov_$1.info"
    # Wait for the container to stop
    docker wait ${CONTAINER_NAME_GEAR}
    mv ${CORPUS_DIR}/coverage_$1.txt ${ARCHIVE_PATH}
    mv ${CORPUS_DIR}/lcov_$1.txt ${ARCHIVE_PATH}
    # Clear folder with corpus
    rm -rf $WORK_DIR/corpus/*
    # Generate new first seed
    dd if=/dev/urandom of=$WORK_DIR/corpus/first-seed bs=1 count=350000
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
        	-v "${CORPUS_DIR}:/corpus/main" \
        	-v "${ARTIFACT_DIR}:/gear/utils/runtime-fuzzer/fuzz/artifactis/main" \
        	--name ${CONTAINER_NAME} ${IMAGE}
    fi
    # Wait for the container to stop
    docker wait node-fuzzer
}

function alert_tg {
    docker logs -f --tail 100 ${CONTAINER_NAME} >& node-fuzzer.log
    text_message="<b>Node Fuzzer Alert</b> 🔥

<b>Problem:</b> Node Fuzzer Container terminated due to an error.
Please check logs.

<b>Archive link:</b>
<a href='$URL/$1'>Link</a>"

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

function info_alert_tg {
    COV_NUM=$(find $ARCHIVE_PATH -name "gear*" -type f -mmin -720 | wc -l)
    if [[ $COV_NUM -gt 0 ]]; then
    text_message="❕<b>Node Fuzzer Info</b>❕
<b>Info:</b> For last 12 hours <b>$COV_NUM</b> coverages was created.
<b>Link:</b> <a href='$URL/'>Link</a>
<b>Disc Free Space:</b> $(df -h | grep root | awk '{print $4}')"
    curl -s \
        --data "text=$text_message" --data "chat_id=$GROUP_ID" \
        'https://api.telegram.org/bot'$BOT_TOKEN'/sendMessage?parse_mode=HTML'
    fi
}

function archive_logs {
    ARCHIVE_NAME="node-fuzzer_logs_$1.tar.gz"
    # Get the logs from the container and archive them with the current timestamp in the filename
    docker logs node-fuzzer >& node-fuzzer.log
    split -C 1024m --additional-suffix=.log --numeric-suffixes node-fuzzer.log node-fuzzer_part
    rm node-fuzzer.log
    #echo "Copy fuzzing-seeds"
    #cp ${VOLUME_DIR}fuzzing-seeds ./
    echo "Creating tar archive: ${ARCHIVE_NAME}"
    # Tar logs and seeds
    tar -czvf ${ARCHIVE_PATH}/${ARCHIVE_NAME} *.log corpus
    # Clean temp files
    echo "Clean tmp files"
    rm *.log
    rm -rf ./atrifacts/*
}

function start {
    # Loop to keep restarting the container if it stops due to an error
    while true; do
        echo "########## $(date) ###########" 
        echo "Start container: ${CONTAINER_NAME}"
        start_container
        DATE=$(date +%Y-%m-%d_%H-%M-%S)
        _check_container_runtime
        if ! _check_stop_manually; then
            echo "start archive"
	    # Create archive
            archive_logs $DATE
            if ! _check_stop_oom; then
                # Create telegram alert
                alert_tg $ARCHIVE_NAME
            fi
            start_container_post $DATE
        fi
    # Clean up the container
    docker rm ${CONTAINER_NAME}        
    # Clean up image
    docker rmi ${IMAGE}
	# Clean archives older than 30 days
	find ${ARCHIVE_PATH} -name "node-fuzzer_logs*.tar.gz" -type f -mtime +2 -delete 
    done
}

function stop {
    # Stop the container
    docker stop ${CONTAINER_NAME}
    # Clean up the container
    docker rm ${CONTAINER_NAME}
    docker rmi ${IMAGE}
}

function info {
    info_alert_tg
}

case "$1" in 
    start)   start;;
    stop)    stop;;
    info)    info;;
    *) echo "usage: $0 start_app|stop_app" >&2
       exit 1
       ;;
esac

