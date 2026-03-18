#!/usr/bin/env bash

#
# This file is part of Gear.
#
# Copyright (C) 2024-2025 Gear Technologies Inc.
# SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.
#

set -euo pipefail

# Configuration
NUM_VALIDATORS="5"

BASE_DIR="/tmp/ethexe-local"
CLEAN_NODE_DATA_ON_START="true"
CLEANUP_DATA="false"

NETWORK_PORT_START="20333"
RPC_PORT_START="10000"
PROMETHEUS_PORT_START="11000"

ANVIL_PORT="8545"
ANVIL_BLOCK_TIME="2"
ANVIL_CONTAINER_NAME="ethexe-anvil"

DOCKER_NETWORK_NAME="ethexe-local-net"
ETHEXE_NODE_IMAGE="rust:1-trixie"
ANVIL_IMAGE="ghcr.io/foundry-rs/foundry:latest"
NODE_CONTAINER_PREFIX="ethexe-node"

CONTAINER_NETWORK_PORT="20333"
CONTAINER_RPC_PORT="9944"
CONTAINER_PROMETHEUS_PORT="9635"

ETHEXE_CLI="target/release/ethexe"
ETHEXE_CLI_IN_CONTAINER="/workspace/target/release/ethexe"

CONTRACTS_DIR="ethexe/contracts"

ENABLE_CHAOS_MODE="false"
CHAOS_INTERVAL="60"

ENABLE_NODE_LOADER="false"
NODE_LOADER_WORKERS="3"
NODE_LOADER_BIN="target/release/ethexe-node-loader"
NODE_LOADER_BIN_IN_CONTAINER="/workspace/target/release/ethexe-node-loader"
NODE_LOADER_CONTAINER_NAME="ethexe-node-loader"
NODE_LOADER_IMAGE="rust:1-trixie"
NODE_LOADER_BATCH_SIZE="5"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

# Prefunded accounts from node-loader/src/main.rs.

declare -a VALIDATOR_PRIVATE_KEYS=(
	"0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a"
	"0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6"
	"0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a"
	"0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba"
	"0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e"
	"0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356"
	"0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97"
	"0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6"
	"0xf214f2b2cd398c806f84e317254e0f0b801d0643303237d97a22a48e01628897"
	"0x701b615bbdfb9de65240bc28bd21bbc0d996645a3dd57e7b12bc2bdf6f192c82"
	"0xa267530f49f8280200edf313ee7af6b827f2a8bce2897751d06a843f644967b1"
	"0x47c99abed3324a2707c28affff1267e45918ec8c3f20b8aa892e8b065d2942dd"
	"0xc526ee95bf44d8fc405a158bb884d9d1238d99f0612e9f33d006bb0789009aaa"
	"0x8166f546bab6da521a8369cab06c5d2b9e46670292d85c875ee9ec20e84ffb61"
	"0xea6c44ac03bff858b476bba40716402b03e41b8e97e276d1baec7c37d42484a0"
	"0x689af8efa8c651a91ad287602527f3af2fe9f6501a7ac4b061667b5a93e037fd"
	"0xde9be858da4a475276426320d5e9262ecfc3ba460bfac56360bfa6c4c28b4ee0"
	"0xdf57089febbacf7ba0bc227dafbffa9fc08a93fdc68e1e42411a14efcf23656e"
	"0xeaa861a9a01391ed3d587d8a5a84ca56ee277629a8b02c22093a419bf240e65d"
	"0xc511b2aa70776d4ff1d376e8537903dae36896132c90b91d52c1dfbae267cd8b"
	"0x224b7eb7449992aac96d631d9677f7bf5888245eef6d6eeda31e62d2f29a83e4"
	"0x4624e0802698b9769f5bdb260a3777fbd4941ad2901f5966b854f953497eec1b"
	"0x375ad145df13ed97f8ca8e27bb21ebf2a3819e9e0a06509a812db377e533def7"
	"0x18743e59419b01d1d846d97ea070b5a3368a3e7f6f0242cf497e1baac6972427"
	"0xe383b226df7c8282489889170b0f68f66af6459261f4833a781acd0804fafe7a"
	"0xf3a6b71b94f5cd909fb2dbb287da47badaa6d8bcdc45d595e2884835d8749001"
	"0x4e249d317253b9641e477aba8dd5d8f1f7cf5250a5acadd1229693e262720a19"
	"0x233c86e887ac435d7f7dc64979d7758d69320906a0d340d2b6518b0fd20aa998"
	"0x85a74ca11529e215137ccffd9c95b2c72c5fb0295c973eb21032e823329b3d2d"
	"0xac8698a440d33b866b6ffe8775621ce1a4e6ebd04ab7980deb97b3d997fc64fb"
	"0xf076539fbce50f0513c488f32bf81524d30ca7a29f400d68378cc5b1b17bc8f2"
	"0x5544b8b2010dbdbef382d254802d856629156aba578f453a76af01b81a80104e"
	"0x47003709a0a9a4431899d4e014c1fd01c5aad19e873172538a02370a119bae11"
	"0x9644b39377553a920edc79a275f45fa5399cbcf030972f771d0bca8097f9aad3"
	"0xcaa7b4a2d30d1d565716199f068f69ba5df586cf32ce396744858924fdf827f0"
	"0xfc5a028670e1b6381ea876dd444d3faaee96cffae6db8d93ca6141130259247c"
	"0x5b92c5fe82d4fabee0bc6d95b4b8a3f9680a0ed7801f631035528f32c9eb2ad5"
	"0xb68ac4aa2137dd31fd0732436d8e59e959bb62b4db2e6107b15f594caf0f405f"
	"0xc95eaed402c8bd203ba04d81b35509f17d0719e3f71f40061a2ec2889bc4caa7"
	"0x55afe0ab59c1f7bbd00d5531ddb834c3c0d289a4ff8f318e498cb3f004db0b53"
	"0xc3f9b30f83d660231203f8395762fa4257fa7db32039f739630f87b8836552cc"
	"0x3db34a7bcc6424e7eadb8e290ce6b3e1423c6e3ef482dd890a812cd3c12bbede"
	"0xae2daaa1ce8a70e510243a77187d2bc8da63f0186074e4a4e3a7bfae7fa0d639"
	"0x5ea5c783b615eb12be1afd2bdd9d96fae56dda0efe894da77286501fd56bac64"
	"0xf702e0ff916a5a76aaf953de7583d128c013e7f13ecee5d701b49917361c5e90"
	"0x7ec49efc632757533404c2139a55b4d60d565105ca930a58709a1c52d86cf5d3"
	"0x755e273950f5ae64f02096ae99fe7d4f478a28afd39ef2422068ee7304c636c0"
	"0xaf6ecabcdbbfb2aefa8248b19d811234cd95caa51b8e59b6ffd3d4bbc2a6be4c"
)

log_info() {
	printf '\033[0;32m[INFO]\033[0m %s\n' "$1"
}

log_warn() {
	printf '\033[0;33m[WARN]\033[0m %s\n' "$1"
}

log_error() {
	printf '\033[0;31m[ERROR]\033[0m %s\n' "$1"
}

require_cmd() {
	command -v "$1" >/dev/null 2>&1 || {
		log_error "'$1' not found"
		exit 1
	}
}

print_help() {
	cat <<'EOF'
Usage: start-local-network.sh [options]

Starts a local ethexe network with anvil, deploys contracts, and launches validators.

Options:
  -h, --help                              Show this help and exit

  --num-validators N                      Number of validator nodes (default: 5)
  --base-dir PATH                         Data root for per-node directories
                                          (default: /tmp/ethexe-local)
  --clean-node-data-on-start true|false   Remove BASE_DIR/node_* before startup
                                          (default: true)
  --cleanup-data true|false               Remove BASE_DIR on Ctrl+C cleanup
                                          (default: false)

  --network-port-start PORT               Host start port for node p2p (default: 20333)
  --rpc-port-start PORT                   Host start port for node RPC (default: 10000)
  --prometheus-port-start PORT            Host start port for metrics (default: 11000)

  --anvil-port PORT                       Host port mapped to anvil (default: 8545)
  --anvil-block-time SEC                  Anvil block time (default: 2)
  --anvil-container-name NAME             Anvil container name (default: ethexe-anvil)
  --anvil-image IMAGE                     Anvil image (default: ghcr.io/foundry-rs/foundry:latest)

  --docker-network-name NAME              Docker network name (default: ethexe-local-net)
  --ethexe-node-image IMAGE               Validator node image (default: rust:1-trixie)
  --node-container-prefix PREFIX          Node container prefix (default: ethexe-node)

  --container-network-port PORT           Internal node network port (default: 20333)
  --container-rpc-port PORT               Internal node RPC port (default: 9944)
  --container-prometheus-port PORT        Internal node metrics port (default: 9635)

  --ethexe-cli PATH                       Host ethexe CLI path (default: target/release/ethexe)
  --ethexe-cli-in-container PATH          ethexe CLI path inside container
                                          (default: /workspace/target/release/ethexe)
  --contracts-dir PATH                    Contracts dir for forge scripts (default: ethexe/contracts)

  --chaos-mode                            Enable chaos mode: randomly stop/start
                                          validators (default: off)
  --chaos-interval SEC                    Seconds between chaos actions (default: 60)

  --node-loader                           Start node-loader (default: off)
  --node-loader-workers N                 Node-loader workers (default: 3)
  --node-loader-batch-size N              Node-loader batch size (default: 5)
  --node-loader-bin PATH                  Node-loader binary path (default: target/release/ethexe-node-loader)
  --node-loader-bin-in-container PATH     Node-loader binary path in container
                                          (default: /workspace/target/release/ethexe-node-loader)
  --node-loader-container-name NAME       Node-loader container name (default: ethexe-node-loader)
  --node-loader-image IMAGE               Node-loader image (default: rust:1-trixie)

Example:
  ./ethexe/scripts/start-local-network.sh \
    --num-validators 5 \
    --node-loader \
    --chaos-mode \
    --clean-node-data-on-start true
EOF
}

require_option_value() {
	local option="$1"
	local value="${2:-}"
	if [[ -z "$value" || "$value" == --* ]]; then
		log_error "Option '$option' requires a value"
		exit 1
	fi
}

parse_args() {
	while [[ $# -gt 0 ]]; do
		case "$1" in
		-h | --help)
			print_help
			exit 0
			;;
		--num-validators)
			require_option_value "$1" "${2:-}"
			NUM_VALIDATORS="$2"
			shift 2
			;;
		--base-dir)
			require_option_value "$1" "${2:-}"
			BASE_DIR="$2"
			shift 2
			;;
		--clean-node-data-on-start)
			require_option_value "$1" "${2:-}"
			CLEAN_NODE_DATA_ON_START="$2"
			shift 2
			;;
		--cleanup-data)
			require_option_value "$1" "${2:-}"
			CLEANUP_DATA="$2"
			shift 2
			;;
		--network-port-start)
			require_option_value "$1" "${2:-}"
			NETWORK_PORT_START="$2"
			shift 2
			;;
		--rpc-port-start)
			require_option_value "$1" "${2:-}"
			RPC_PORT_START="$2"
			shift 2
			;;
		--prometheus-port-start)
			require_option_value "$1" "${2:-}"
			PROMETHEUS_PORT_START="$2"
			shift 2
			;;
		--anvil-port)
			require_option_value "$1" "${2:-}"
			ANVIL_PORT="$2"
			shift 2
			;;
		--anvil-block-time)
			require_option_value "$1" "${2:-}"
			ANVIL_BLOCK_TIME="$2"
			shift 2
			;;
		--anvil-container-name)
			require_option_value "$1" "${2:-}"
			ANVIL_CONTAINER_NAME="$2"
			shift 2
			;;
		--docker-network-name)
			require_option_value "$1" "${2:-}"
			DOCKER_NETWORK_NAME="$2"
			shift 2
			;;
		--ethexe-node-image)
			require_option_value "$1" "${2:-}"
			ETHEXE_NODE_IMAGE="$2"
			shift 2
			;;
		--anvil-image)
			require_option_value "$1" "${2:-}"
			ANVIL_IMAGE="$2"
			shift 2
			;;
		--node-container-prefix)
			require_option_value "$1" "${2:-}"
			NODE_CONTAINER_PREFIX="$2"
			shift 2
			;;
		--container-network-port)
			require_option_value "$1" "${2:-}"
			CONTAINER_NETWORK_PORT="$2"
			shift 2
			;;
		--container-rpc-port)
			require_option_value "$1" "${2:-}"
			CONTAINER_RPC_PORT="$2"
			shift 2
			;;
		--container-prometheus-port)
			require_option_value "$1" "${2:-}"
			CONTAINER_PROMETHEUS_PORT="$2"
			shift 2
			;;
		--ethexe-cli)
			require_option_value "$1" "${2:-}"
			ETHEXE_CLI="$2"
			shift 2
			;;
		--ethexe-cli-in-container)
			require_option_value "$1" "${2:-}"
			ETHEXE_CLI_IN_CONTAINER="$2"
			shift 2
			;;
		--contracts-dir)
			require_option_value "$1" "${2:-}"
			CONTRACTS_DIR="$2"
			shift 2
			;;
		--chaos-mode)
			ENABLE_CHAOS_MODE="true"
			shift
			;;
		--chaos-interval)
			require_option_value "$1" "${2:-}"
			CHAOS_INTERVAL="$2"
			shift 2
			;;
		--node-loader)
			ENABLE_NODE_LOADER="true"
			shift
			;;
		--node-loader-workers)
			require_option_value "$1" "${2:-}"
			NODE_LOADER_WORKERS="$2"
			shift 2
			;;
		--node-loader-batch-size)
			require_option_value "$1" "${2:-}"
			NODE_LOADER_BATCH_SIZE="$2"
			shift 2
			;;
		--node-loader-bin)
			require_option_value "$1" "${2:-}"
			NODE_LOADER_BIN="$2"
			shift 2
			;;
		--node-loader-bin-in-container)
			require_option_value "$1" "${2:-}"
			NODE_LOADER_BIN_IN_CONTAINER="$2"
			shift 2
			;;
		--node-loader-container-name)
			require_option_value "$1" "${2:-}"
			NODE_LOADER_CONTAINER_NAME="$2"
			shift 2
			;;
		--node-loader-image)
			require_option_value "$1" "${2:-}"
			NODE_LOADER_IMAGE="$2"
			shift 2
			;;
		*)
			log_error "Unknown option: $1"
			echo "Run with --help to see supported options."
			exit 1
			;;
		esac
	done
}

docker_container_exists() {
	docker ps -a --format '{{.Names}}' | grep -Fxq "$1"
}

docker_container_running() {
	docker ps --format '{{.Names}}' | grep -Fxq "$1"
}

remove_container_if_exists() {
	local name="$1"
	if docker_container_exists "$name"; then
		docker rm -t 2 -f "$name" >/dev/null 2>&1 || true
	fi
}

ensure_network() {
	if ! docker network inspect "$DOCKER_NETWORK_NAME" >/dev/null 2>&1; then
		docker network create "$DOCKER_NETWORK_NAME" >/dev/null
		log_info "Created docker network: $DOCKER_NETWORK_NAME"
	fi
}

cleanup_node_data_on_start() {
	if [[ "$CLEAN_NODE_DATA_ON_START" != "true" ]]; then
		log_warn "Skipping node data cleanup (CLEAN_NODE_DATA_ON_START=false)"
		return
	fi

	if [[ ! -d "$BASE_DIR" ]]; then
		return
	fi

	log_info "Cleaning existing node data in $BASE_DIR/node_* to avoid stale genesis DB mismatches..."
	find "$BASE_DIR" -mindepth 1 -maxdepth 1 -type d -name 'node_*' -exec rm -rf {} +
}

cleanup() {
	log_info "Cleaning up..."

	if [[ -n "${CHAOS_PID:-}" ]] && kill -0 "$CHAOS_PID" 2>/dev/null; then
		kill "$CHAOS_PID" 2>/dev/null || true
		wait "$CHAOS_PID" 2>/dev/null || true
	fi

	for container_name in "${NODE_CONTAINER_NAMES[@]:-}"; do
		remove_container_if_exists "$container_name"
	done
	log_info "Stopped all validator node containers"

	remove_container_if_exists "$ANVIL_CONTAINER_NAME"

	remove_container_if_exists "$NODE_LOADER_CONTAINER_NAME"

	if docker network inspect "$DOCKER_NETWORK_NAME" >/dev/null 2>&1; then
		docker network rm "$DOCKER_NETWORK_NAME" >/dev/null 2>&1 || true
	fi

	rm -f ~/.local/share/ethexe/keys/secp/validator-node-*.json 2>/dev/null
	rm -f ~/.local/share/ethexe/net/secp/network-node-*.json 2>/dev/null

	if [[ "${CLEANUP_DATA:-false}" == "true" ]]; then
		rm -rf "$BASE_DIR"
		log_info "Removed data directory: $BASE_DIR"
	fi

	exit 0
}

trap cleanup SIGINT SIGTERM

parse_args "$@"

if [[ $NUM_VALIDATORS -lt 1 ]]; then
	log_error "NUM_VALIDATORS must be at least 1"
	exit 1
fi

if [[ $NUM_VALIDATORS -gt ${#VALIDATOR_PRIVATE_KEYS[@]} ]]; then
	log_error "NUM_VALIDATORS ($NUM_VALIDATORS) exceeds available prefunded accounts (${#VALIDATOR_PRIVATE_KEYS[@]})"
	exit 1
fi

require_cmd jq
require_cmd docker
if ! docker info >/dev/null 2>&1; then
	log_error "docker daemon is not reachable. Please start Docker and try again."
	exit 1
fi

if [[ ! -x "$WORKSPACE_ROOT/$ETHEXE_CLI" ]]; then
	log_error "ethexe binary '$WORKSPACE_ROOT/$ETHEXE_CLI' is not executable"
	exit 1
fi

start_anvil() {
	log_info "Starting anvil on port $ANVIL_PORT with block time ${ANVIL_BLOCK_TIME}s..."

	remove_container_if_exists "$ANVIL_CONTAINER_NAME"

	docker run -d \
		--name "$ANVIL_CONTAINER_NAME" \
		--entrypoint anvil \
		--network "$DOCKER_NETWORK_NAME" \
		-p "$ANVIL_PORT:8545" \
		"$ANVIL_IMAGE" \
		--host 0.0.0.0 \
		--port 8545 \
		--block-time "$ANVIL_BLOCK_TIME" \
		--accounts 72 \
		--mnemonic "test test test test test test test test test test test junk" \
		>/dev/null

	local retries=30
	while [[ $retries -gt 0 ]]; do
		if docker exec "$ANVIL_CONTAINER_NAME" sh -lc "cast block-number --rpc-url http://127.0.0.1:8545 >/dev/null 2>&1"; then
			break
		fi
		sleep 1
		retries=$((retries - 1))
	done

	if [[ $retries -eq 0 ]]; then
		log_error "Anvil failed to become ready in container '$ANVIL_CONTAINER_NAME'"
		docker logs "$ANVIL_CONTAINER_NAME" | tail -n 100
		exit 1
	fi

	log_info "Anvil is running in container: $ANVIL_CONTAINER_NAME"
}

deploy_contracts() {
	log_info "Deploying contracts..."
	local validators_list
	validators_list="$(
		IFS=,
		echo "${VALIDATOR_ADDRESSES[*]}"
	)"

	log_info "Router validators list: $validators_list"

	export PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
	export DEV_MODE="true"
	export ROUTER_AGGREGATED_PUBLIC_KEY_X="0x1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f"
	export ROUTER_AGGREGATED_PUBLIC_KEY_Y="0x70beaf8f588b541507fed6a642c5ab42dfdf8120a7f639de5122d47a69a8e8d1"
	export ROUTER_VERIFIABLE_SECRET_SHARING_COMMITMENT="0x"
	export ROUTER_VALIDATORS_LIST="$validators_list"
	export SENDER_ADDRESS="0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC"

	export IS_POA="true"
	export SYMBIOTIC_VAULT_REGISTRY="0x0000000000000000000000000000000000000000"
	export SYMBIOTIC_OPERATOR_REGISTRY="0x0000000000000000000000000000000000000000"
	export SYMBIOTIC_NETWORK_REGISTRY="0x0000000000000000000000000000000000000000"
	export SYMBIOTIC_MIDDLEWARE_SERVICE="0x0000000000000000000000000000000000000000"
	export SYMBIOTIC_NETWORK_OPT_IN="0x0000000000000000000000000000000000000000"
	export SYMBIOTIC_STAKER_REWARDS_FACTORY="0x0000000000000000000000000000000000000000"
	export SYMBIOTIC_OPERATOR_REWARDS_FACTORY="0x0000000000000000000000000000000000000000"

	(cd "$CONTRACTS_DIR" && forge clean && forge script script/Deployment.s.sol:DeploymentScript --rpc-url "ws://localhost:$ANVIL_PORT" --broadcast)

	BROADCAST_PATH="$CONTRACTS_DIR/broadcast/Deployment.s.sol/31337/run-latest.json"

	if [[ ! -f "$BROADCAST_PATH" ]]; then
		log_error "Contract deployment failed. Check forge output above."
		exit 1
	fi

	ROUTER_IMPLEMENTATION=$(jq -r '.transactions[] | select(.contractName == "Router" and .transactionType == "CREATE") | .contractAddress' "$BROADCAST_PATH")

	ROUTER_PROXY_ADDRESS=$(jq -r ".transactions[] | select(.contractName == \"ERC1967Proxy\" and .transactionType == \"CREATE\" and .arguments != null) | select(.arguments[0] | ascii_downcase | contains(\"${ROUTER_IMPLEMENTATION,,}\")) | .contractAddress" "$BROADCAST_PATH")

	if [[ -z "$ROUTER_PROXY_ADDRESS" ]]; then
		log_error "Failed to extract Router proxy address from deployment"
		exit 1
	fi

	log_info "Router implementation at: $ROUTER_IMPLEMENTATION"
	log_info "Router proxy at: $ROUTER_PROXY_ADDRESS"

	log_info "Calling Router.lookupGenesisHash()..."
	export ROUTER_ADDRESS="$ROUTER_PROXY_ADDRESS"
	(cd "$CONTRACTS_DIR" && forge script script/LookupGenesisHash.s.sol:LookupGenesisHashScript --slow --rpc-url "ws://localhost:$ANVIL_PORT" --broadcast -vvvv)

	log_info "Genesis hash lookup complete"

	export ROUTER_ADDRESS="$ROUTER_PROXY_ADDRESS"
}

declare -a VALIDATOR_PUB_KEYS=()
declare -a VALIDATOR_ADDRESSES=()
declare -a NETWORK_PUB_KEYS=()
declare -a PEER_IDS=()
declare -a NODE_CONTAINER_NAMES=()

generate_keys() {
	log_info "Generating validator keys for $NUM_VALIDATORS nodes..."

	mkdir -p "$BASE_DIR"
	mkdir -p ~/.local/share/ethexe/keys/secp
	mkdir -p ~/.local/share/ethexe/net/secp

	for ((i = 0; i < NUM_VALIDATORS; i++)); do
		log_info "Generating keys for node $i..."
		local private_key="${VALIDATOR_PRIVATE_KEYS[$i]}"
		local node_dir="$BASE_DIR/node_$i"
		local keys_dir="$node_dir/keys/secp"
		local net_dir="$node_dir/net/secp"
		mkdir -p "$keys_dir"
		mkdir -p "$net_dir"

		local validator_result
		validator_result=$("$ETHEXE_CLI" key keyring import \
			--private-key "$private_key" \
			--name "validator-node-$i" \
			--show-secret 2>&1)
		local validator_pub_key
		validator_pub_key=$(echo "$validator_result" | grep "Public key:" | awk '{print $3}')
		local validator_address
		validator_address=$(echo "$validator_result" | grep "Address:" | awk '{print $2}')

		if [[ -z "$validator_pub_key" ]]; then
			log_error "Failed to generate validator key for node $i"
			echo "$validator_result"
			exit 1
		fi

		VALIDATOR_PUB_KEYS+=("$validator_pub_key")
		VALIDATOR_ADDRESSES+=("$validator_address")
		log_info "Node $i validator key: $validator_pub_key address: $validator_address"

		cp ~/.local/share/ethexe/keys/secp/validator-node-$i.json "$keys_dir/validator.json"

		local network_result
		network_result=$("$ETHEXE_CLI" key --net keyring import \
			--private-key "$private_key" \
			--name "network-node-$i" \
			--show-secret 2>&1)
		local network_pub_key
		network_pub_key=$(echo "$network_result" | grep "Public key:" | awk '{print $3}')

		if [[ -z "$network_pub_key" ]]; then
			log_error "Failed to generate network key for node $i"
			echo "$network_result"
			exit 1
		fi

		NETWORK_PUB_KEYS+=("$network_pub_key")

		cp ~/.local/share/ethexe/net/secp/network-node-$i.json "$net_dir/network.json"

		local peer_id_result
		peer_id_result=$("$ETHEXE_CLI" key --net peer-id --public-key "$network_pub_key" 2>&1)
		local peer_id
		peer_id=$(echo "$peer_id_result" | grep "PeerId:" | awk '{print $2}')

		if [[ -z "$peer_id" ]]; then
			log_error "Failed to derive Peer ID for node $i"
			echo "$peer_id_result"
			exit 1
		fi

		PEER_IDS+=("$peer_id")
		log_info "Node $i: validator=$validator_pub_key peer_id=$peer_id"
	done
}

start_nodes() {
	log_info "Starting $NUM_VALIDATORS validator nodes..."

	export RUST_LOG_STYLE=never
	for ((i = 0; i < NUM_VALIDATORS; i++)); do
		local node_dir="$BASE_DIR/node_$i"
		local network_port=$((NETWORK_PORT_START + i))
		local rpc_port=$((RPC_PORT_START + i))
		local prometheus_port=$((PROMETHEUS_PORT_START + i))
		local validator_pub_key="${VALIDATOR_PUB_KEYS[$i]}"
		local network_pub_key="${NETWORK_PUB_KEYS[$i]}"
		local container_name="${NODE_CONTAINER_PREFIX}-${i}"
		NODE_CONTAINER_NAMES+=("$container_name")

		log_info "Starting node $i on ports: network=$network_port rpc=$rpc_port prometheus=$prometheus_port"

		remove_container_if_exists "$container_name"

		local cmd="$ETHEXE_CLI_IN_CONTAINER run"
		cmd+=" --base /data"
		cmd+=" --validator $validator_pub_key"
		cmd+=" --validator-session $validator_pub_key"
		cmd+=" --network-key $network_pub_key"
		cmd+=" --rpc-external"
		cmd+=" --ethereum-rpc ws://$ANVIL_CONTAINER_NAME:8545"
		cmd+=" --ethereum-beacon-rpc http://$ANVIL_CONTAINER_NAME:8545"
		cmd+=" --ethereum-router $ROUTER_ADDRESS"
		cmd+=" --rpc-port $CONTAINER_RPC_PORT"
		cmd+=" --prometheus-port $CONTAINER_PROMETHEUS_PORT"
		cmd+=" --canonical-quarantine 0"
		cmd+=" --net-listen-addr /ip4/0.0.0.0/udp/$CONTAINER_NETWORK_PORT/quic-v1"

		if [[ $i -gt 0 ]]; then
			for ((j = 0; j < i; j++)); do
				local bootnode_peer_id="${PEER_IDS[$j]}"
				local bootnode_container="${NODE_CONTAINER_PREFIX}-${j}"
				cmd+=" --network-bootnodes /dns4/$bootnode_container/udp/$CONTAINER_NETWORK_PORT/quic-v1/p2p/$bootnode_peer_id"
			done
		fi

		docker run -d \
			--name "$container_name" \
			--network "$DOCKER_NETWORK_NAME" \
			-p "$network_port:$CONTAINER_NETWORK_PORT/udp" \
			-p "$rpc_port:$CONTAINER_RPC_PORT" \
			-p "$prometheus_port:$CONTAINER_PROMETHEUS_PORT" \
			-e RUST_LOG_STYLE=never \
			-e RUST_BACKTRACE=1 \
			-v "$WORKSPACE_ROOT:/workspace" \
			-v "$node_dir:/data" \
			-w /workspace \
			"$ETHEXE_NODE_IMAGE" \
			bash -lc "$cmd" >/dev/null

		log_info "Node $i started in container: $container_name (log: docker logs $container_name)"

		sleep 1
	done

	log_info "All $NUM_VALIDATORS nodes started"
}

start_node_loader() {
	if [[ "$ENABLE_NODE_LOADER" != "true" ]]; then
		return
	fi

	log_info "Starting node-loader with $NODE_LOADER_WORKERS workers..."

	if [[ ! -x "$WORKSPACE_ROOT/$NODE_LOADER_BIN" ]]; then
		log_error "Node-loader binary not found at '$NODE_LOADER_BIN'. Build it with: cargo build --release -p node-loader"
		exit 1
	fi

	local ethexe_nodes=""
	for ((i = 0; i < NUM_VALIDATORS; i++)); do
		if [[ -n "$ethexe_nodes" ]]; then
			ethexe_nodes+=","
		fi
		ethexe_nodes+="ws://${NODE_CONTAINER_PREFIX}-${i}:$CONTAINER_RPC_PORT"
	done

	local anvil_url="ws://$ANVIL_CONTAINER_NAME:8545"

	log_info "Node-loader will use ethexe nodes: $ethexe_nodes"

	remove_container_if_exists "$NODE_LOADER_CONTAINER_NAME"

	local cmd="$NODE_LOADER_BIN_IN_CONTAINER load"
	cmd+=" --node $anvil_url"
	cmd+=" --ethexe-node $ethexe_nodes"
	cmd+=" --router-address $ROUTER_ADDRESS"
	cmd+=" --workers $NODE_LOADER_WORKERS"
	cmd+=" --batch-size $NODE_LOADER_BATCH_SIZE"

	docker run -d \
		--name "$NODE_LOADER_CONTAINER_NAME" \
		--network "$DOCKER_NETWORK_NAME" \
		-e RUST_LOG=debug,alloy_rpc_client=off,alloy_provider=off,alloy_pubsub=off \
		-e RUST_LOG_STYLE=never \
		-v "$WORKSPACE_ROOT:/workspace" \
		-w /workspace \
		"$NODE_LOADER_IMAGE" \
		bash -lc "$cmd" >/dev/null

	log_info "Node-loader started in container: $NODE_LOADER_CONTAINER_NAME (log: docker logs -f $NODE_LOADER_CONTAINER_NAME)"
}

chaos_loop() {
	if [[ "$ENABLE_CHAOS_MODE" != "true" ]]; then
		return
	fi

	if [[ $NUM_VALIDATORS -lt 2 ]]; then
		log_warn "Chaos mode requires at least 2 validators (bootnode is excluded)"
		return
	fi

	log_info "Chaos mode enabled (interval=${CHAOS_INTERVAL}s). Bootnode (node 0) is excluded."

	while true; do
		sleep "$CHAOS_INTERVAL"

		local target=$(((RANDOM % (NUM_VALIDATORS - 1)) + 1))
		local container_name="${NODE_CONTAINER_PREFIX}-${target}"

		if docker_container_running "$container_name"; then
			log_warn "[CHAOS] Stopping node $target ($container_name)..."
			docker stop -t 2 "$container_name" >/dev/null 2>&1 || true

			sleep "$CHAOS_INTERVAL"

			log_info "[CHAOS] Restarting node $target ($container_name)..."
			docker start "$container_name" >/dev/null 2>&1 || true
		else
			log_info "[CHAOS] Node $target ($container_name) already stopped, restarting..."
			docker start "$container_name" >/dev/null 2>&1 || true
		fi
	done
}

print_summary() {
	echo ""
	echo "================================================================================"
	echo "                           LOCAL NETWORK SUMMARY                                "
	echo "================================================================================"
	echo ""
	echo "Anvil (Ethereum):"
	echo "  RPC URL:  ws://localhost:$ANVIL_PORT"
	echo "  Container: $ANVIL_CONTAINER_NAME"
	echo "  Logs:      docker logs -f $ANVIL_CONTAINER_NAME"
	echo ""
	echo "Router Contract: $ROUTER_ADDRESS"
	echo ""
	echo "Validator Nodes:"
	for ((i = 0; i < NUM_VALIDATORS; i++)); do
		echo ""
		echo "  Node $i:"
		echo "    Validator Key: ${VALIDATOR_PUB_KEYS[$i]}"
		echo "    Peer ID:       ${PEER_IDS[$i]}"
		echo "    Network Port:  $((NETWORK_PORT_START + i))"
		echo "    RPC Port:      $((RPC_PORT_START + i))"
		echo "    Prometheus:    $((PROMETHEUS_PORT_START + i))"
		echo "    Container:     ${NODE_CONTAINER_NAMES[$i]}"
		echo "    Start:         docker start ${NODE_CONTAINER_NAMES[$i]}"
		echo "    Stop:          docker stop ${NODE_CONTAINER_NAMES[$i]}"
		echo "    Logs:          docker logs -f ${NODE_CONTAINER_NAMES[$i]}"
	done
	echo ""
	if [[ "$ENABLE_CHAOS_MODE" == "true" ]]; then
		echo "Chaos Mode:"
		echo "  Enabled:   true"
		echo "  Interval:  ${CHAOS_INTERVAL}s"
		echo "  Excluded:  node 0 (bootnode)"
		echo ""
	fi
	if [[ "$ENABLE_NODE_LOADER" == "true" ]]; then
		echo "Node-Loader:"
		echo "  Mode:      container"
		echo "  Container: $NODE_LOADER_CONTAINER_NAME"
		echo "  Image:     $NODE_LOADER_IMAGE"
		echo "  Logs:      docker logs -f $NODE_LOADER_CONTAINER_NAME"
		echo "  Workers:   $NODE_LOADER_WORKERS"
		echo "  Batch Size: $NODE_LOADER_BATCH_SIZE"
		echo ""
	fi
	echo "================================================================================"
	echo ""
	echo "To stop all nodes and anvil:"
	echo "  docker rm -f $ANVIL_CONTAINER_NAME ${NODE_CONTAINER_NAMES[*]} $NODE_LOADER_CONTAINER_NAME"
	echo ""
	echo "To tail logs of a specific node: docker logs -f ${NODE_CONTAINER_PREFIX}-<N>"
	echo ""
}

main() {
	log_info "Starting local ethexe network with $NUM_VALIDATORS validators"
	log_info "Base directory: $BASE_DIR"

	remove_container_if_exists "$ANVIL_CONTAINER_NAME"
	remove_container_if_exists "$NODE_LOADER_CONTAINER_NAME"
	for ((i = 0; i < NUM_VALIDATORS; i++)); do
		remove_container_if_exists "${NODE_CONTAINER_PREFIX}-${i}"
	done

	ensure_network

	cleanup_node_data_on_start

	mkdir -p "$BASE_DIR"

	start_anvil

	generate_keys

	deploy_contracts

	start_nodes

	start_node_loader

	print_summary

	log_info "Network is ready. Press Ctrl+C to stop/remove all containers."

	chaos_loop &
	CHAOS_PID=$!

	while true; do
		sleep 60
	done
}

main
