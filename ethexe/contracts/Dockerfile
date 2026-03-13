FROM ubuntu:24.04 AS builder
# required
ARG PRIVATE_KEY
ARG ROUTER_AGGREGATED_PUBLIC_KEY_X
ARG ROUTER_AGGREGATED_PUBLIC_KEY_Y
ARG ROUTER_VERIFIABLE_SECRET_SHARING_COMMITMENT
ARG ROUTER_VALIDATORS_LIST
# optional
ARG SENDER_ADDRESS
ARG IS_POA="true"
# constants
ARG RPC_URL="http://127.0.0.1:8545"
ARG PREDEFINED_PRIVATE_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
WORKDIR /contracts
COPY . .
ENV PATH="${PATH}:/root/.foundry/bin"
ENV NVM_DIR="/root/.nvm"
RUN test -n "${PRIVATE_KEY}" || { echo "PRIVATE_KEY is required" >&2; exit 1; } && \
    test -n "${ROUTER_AGGREGATED_PUBLIC_KEY_X}" || { echo "ROUTER_AGGREGATED_PUBLIC_KEY_X is required" >&2; exit 1; } && \
    test -n "${ROUTER_AGGREGATED_PUBLIC_KEY_Y}" || { echo "ROUTER_AGGREGATED_PUBLIC_KEY_Y is required" >&2; exit 1; } && \
    test -n "${ROUTER_VERIFIABLE_SECRET_SHARING_COMMITMENT}" || { echo "ROUTER_VERIFIABLE_SECRET_SHARING_COMMITMENT is required" >&2; exit 1; } && \
    test -n "${ROUTER_VALIDATORS_LIST}" || { echo "ROUTER_VALIDATORS_LIST is required" >&2; exit 1; } && \
    apt update && \
    apt install --no-install-recommends --yes \
        ca-certificates \
        curl \
        git \
        jq && \
    rm -rf /var/lib/apt/lists/* && \
    curl -L https://foundry.paradigm.xyz | bash && \
    foundryup && \
    forge --version && \
    forge compiler resolve && \
    curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.4/install.sh | bash && \
    \. "$NVM_DIR/nvm.sh" && \
    nvm install --lts && \
    npm install -g npm@latest && \
    node --version && \
    npm --version && \
    npx @openzeppelin/upgrades-core@^1.37.0 && \
    ( \
        anvil --dump-state ./chain-state.json & \
        ANVIL_PID=$! && \
        cast block-number --rpc-url $RPC_URL && \
        # 100 ETH in wei for each address
        cast send \
            --rpc-url $RPC_URL \
            --value 100000000000000000000 \
            --private-key $PREDEFINED_PRIVATE_KEY \
            $(cast wallet addr --private-key $PRIVATE_KEY) && \
        test -n "$SENDER_ADDRESS" && cast send \
            --rpc-url $RPC_URL \
            --value 100000000000000000000 \
            --private-key $PREDEFINED_PRIVATE_KEY \
            $SENDER_ADDRESS || true && \
        echo "$ROUTER_VALIDATORS_LIST" | tr ',' '\n' | \
        while read VALIDATOR_ADDRESS; do \
            cast send \
                --rpc-url $RPC_URL \
                --value 100000000000000000000 \
                --private-key $PREDEFINED_PRIVATE_KEY \
                "$VALIDATOR_ADDRESS" || true; \
        done && \
        ./deploy.sh $RPC_URL && \
        sleep 5 && \
        kill -2 $ANVIL_PID && \
        wait $ANVIL_PID \
    )

FROM ubuntu:24.04
WORKDIR /vara-eth-ethereum-node
COPY --from=builder /contracts/chain-state.json .
COPY --from=builder /root/.foundry/bin/anvil /usr/local/bin/anvil
EXPOSE 8545
CMD ["anvil", "--block-time", "12", "--load-state", "./chain-state.json", "--host", "0.0.0.0"]
