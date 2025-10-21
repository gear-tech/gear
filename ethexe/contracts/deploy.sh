# Deployment Vara.eth contracts
export RPC="https://hoodi-reth-rpc.gear-tech.io"
export PRIVATE_KEY="0x26f7a24ff7e226a118ea4eea21a12d9cc3bd43033a7efddf415f83d8bd58a98b"


forge script script/Deployment.s.sol:DeploymentScript \
    --rpc-url $RPC --broadcast --private-key $PRIVATE_KEY --slow