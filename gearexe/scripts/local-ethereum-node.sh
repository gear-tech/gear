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

set -eu

# never used
export ETHERSCAN_API_KEY=""
# anvil account (0) with balance
export PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
# not yet used
export ROUTER_VALIDATORS_LIST=""

ANVIL_LOG_FILE="/tmp/gearexe-anvil.log"
nohup anvil --block-time 2 > $ANVIL_LOG_FILE 2>&1 &
ANVIL_PID=$!

(cd gearexe/contracts && forge clean && forge script script/Router.s.sol:RouterScript --rpc-url "ws://localhost:8545" --broadcast)

BROADCAST_PATH="gearexe/contracts/broadcast/Router.s.sol/31337/run-latest.json"
ROUTER_ADDRESS=`cat $BROADCAST_PATH | jq '.transactions[] | select(.contractName == "Router") | .contractAddress' | tr -d '"'`
PROXY_ADDRESS=`cat $BROADCAST_PATH | 
  jq ".transactions[] | \
  select(.contractName == \"TransparentUpgradeableProxy\") | \
  select(.transactionType == \"CREATE\") | \
  select(.arguments[] | ascii_downcase | contains(\"${ROUTER_ADDRESS}\")) | \
  .contractAddress" | 
  tr -d '"'`

if [[ -e .gearexe.toml ]]; then
  SED_FROM="ethereum_router_address = \"[a-zA-Z0-9]*\""
  SED_TO="ethereum_router_address = \"${PROXY_ADDRESS}\""
  sed -i.bak "s/$SED_FROM/$SED_TO/g" .gearexe.toml
  rm .gearexe.toml.bak
  
  echo "Router address has been updated in .gearexe.toml"
fi

echo "Router address: ${ROUTER_ADDRESS}"
echo "Proxy address: ${PROXY_ADDRESS}"
echo
echo "Anvil is running via nohup. PID: ${ANVIL_PID}. Logs at $ANVIL_LOG_FILE"
echo
read -p "Press enter to see node logs in real time"

tail -f $ANVIL_LOG_FILE
