// Import the API
const { ApiPromise, Keyring, WsProvider } = require('@polkadot/api');
const { readFileSync } = require('fs');

async function main() {
    console.log(process.argv);
    const endpoint = process.argv[2];
    const seed = process.argv[3];
    const code = process.argv[4].includes('compressed')
        ? '0x' + readFileSync(process.argv[4]).toString('hex')
        : readFileSync(process.argv[4]);

    const provider = new WsProvider(endpoint);

    const api = await ApiPromise.create({ provider });

    const keyring = new Keyring({ type: 'sr25519' });

    const root = keyring.addFromMnemonic(seed);

    const proposal = api.tx.system.setCode(code)

    console.log(`Upgrading from ${root.address}, ${code.length / 2} bytes`);

    const txs = [
        api.tx.sudo.sudoUncheckedWeight(api.tx.system.setCode(proposal), 1),
        api.tx.sudo.sudo(api.tx.gear.reset()),
    ]

    api.tx.utility.batch(txs).signAndSend(root, ({ events = [], status }) => {
        console.log('Proposal status:', status.type);

        if (status.isInBlock) {
            console.log('You have just upgraded your chain');

            console.log('Included at block hash', status.asInBlock.toHex());
            console.log('Events:');

            console.log(JSON.stringify(events, null, 2));
        } else if (status.isFinalized) {
            console.log('Finalized block hash', status.asFinalized.toHex());

            process.exit(0);
        }
    });
}

main()
    .catch((err) => {
        console.error(err);
        process.exit(1);
    });