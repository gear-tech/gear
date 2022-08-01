# Gear Node deployment scripts

## Testnet (Staging)

### Generate custom chain spec

The compiled runtime code as well as the genesis block configuration (the initial network state) should be placed in a "chain spec" JSON file that would later be supplied as a command line argumnet to the ```gear-node``` command:
```bash
./target/release/gear-node \
  --base-path /tmp/data \
  --chain=chain_spec.json \
  ...
```

A custom chain spec can be created in a few simple steps:

1. Build the node as per ususal.
  ```bash
  cargo build --release
  ```
2. Export the chain spec as a [raw] JSON.\
Note that we specifically request not to export the bootnodnes - this section will be filled out manually later.
```bash
./target/release/gear-node build-spec --raw --disable-default-bootnode --chain staging > staging.json
```
The resulting ```staging.json``` file will contain a very long segment of binary data - the runtime wasm code, and will look similar to this:
```
{
  "name": "Staging Testnet V2",
  "id": "staging_testnet_v2",
  "chainType": "Live",
  "bootNodes": [],
  "telemetryEndpoints": null,
  "protocolId": null,
  "properties": null,
  "consensusEngine": null,
  "codeSubstitutes": {},
  "genesis": {
    "raw": {
      "top": {
        "0xd5e1a2fa16732ce6906189438c0a82c64e7b9012096b41c4eb3aaf947f6ea429": "0x0000",
        "0x57f8dc2f5ab09467896f47300f0424384e7b9012096b41c4eb3aaf947f6ea429": "0x0000",
        "0x3f1467a096bcd71a5b6a0c8155e208104e7b9012096b41c4eb3aaf947f6ea429": "0x0000",
        ...
        "0xbd2a529379475088d3e29a918cd478724e7b9012096b41c4eb3aaf947f6ea429": "0x0000"
      },
      "childrenDefault": {}
    }
  }
}
```

3. Manually edit the chain spec JSON file adding the details of the bootnodes.\
This is a necessary piece of information every node that is being started using this chain spec will need: it tells the node what is the initial set of peers it should connect to.\
For the ```staging``` testnet we list the initial validators set as the bootnodes:
```bash
{
  "name": "Staging Testnet V2",
  "id": "staging_testnet_v2",
  "chainType": "Live",
  "bootNodes": [
    "/ip4/52.9.232.93/tcp/30333/p2p/12D3KooWBWFtZqigVTC8W2GRMwLeuTK2o4hDC4XHVPyNV6hW1T1D",
    "/ip4/50.18.102.12/tcp/30333/p2p/12D3KooWRf7vAr79yAyDxGvYAdSqhh2EoeWe35Lx4QH4N6XMv2gH",
    "/ip4/54.153.5.48/tcp/30333/p2p/12D3KooWEVvqVD2mrLfmgeX1EXZ2caFXXEWWEs4Taa4mWzFUoF34",
    "/ip4/54.183.129.20/tcp/30333/p2p/12D3KooWSf2d69w7RYKtj9mgYpLDs3rqLAz9GHNSHHoCQDLUjeiP"
  ],
  "telemetryEndpoints": null,
  "protocolId": null,
  ...
}
```

Upon completion, replace the old chain spec under ```./node/res/staging.json```, commit the changes to the repo and restart the nodes with the updated chain spec.

A more detailed description of this procedure can be found in the Substrate Knowledge Base https://substrate.dev/docs/en/tutorials/start-a-private-network/customspec.
