// Required imports
const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const fs = require('fs');

async function processTest(test, api, alice, nonce) {
    let programs_wasm = [];
    let programs_id = [];

    test.programs.forEach(async (program) => {
        binary = fs.readFile(program.path, (bin) => {
            api.tx
            .gearModule.submitProgram(bin, 0, 2000000)
            .signAndSend(alice, ({ events = [], status }) => {
                console.log('Transaction status:', status.type);

                if (status.isInBlock) {
                    console.log('Included at block hash', status.asInBlock.toHex());
                    console.log('Events:');

                    events.forEach(({ event: { data, method, section }, phase }) => {
                        if (section === 'gearModule' && method === 'NewProgram') {
                            program.program_id = data[0];
                            console.log('\t', phase.toString(), `: ${section}.${method}`, data.toString());
                        }
                    });
                } else if (status.isFinalized) {
                    console.log('Finalized block hash', status.asFinalized.toHex());
                    console.log(program);
                    // api.tx
                    //     .gearModule.sendMessage(program.program_id, "PING", 100000000)
                    //     .signAndSend(alice, { nonce }, ({ events = [], status }) => { 

                    //     });
                }
            }).then(console.log(program));
        });
        // console.log(api.tx.gearModule.submitProgram.meta.args);
        // console.log(Uint8Array.from(binary));

        console.log(program)
    });





    // Sign and send the transaction using our account
    // transfer.signAndSend(alice, { nonce }, ({ events = [], status }) => {
    //     console.log('Transaction status:', status.type);

    //     if (status.isInBlock) {
    //         console.log('Included at block hash', status.asInBlock.toHex());
    //         console.log('Events:');

    //         events.forEach(({ event: { data, method, section }, phase }) => {
    //             console.log('\t', phase.toString(), `: ${section}.${method}`, data.toString());
    //         });
    //     } else if (status.isFinalized) {
    //         console.log('Finalized block hash', status.asFinalized.toHex());

    //         //   process.exit(0);
    //     }
    // });
}

async function main() {
    console.log(process.argv.slice(2));
    let tests = [];

    // Load json files
    process.argv.slice(2).forEach(path => {
        const fileContents = fs.readFileSync(path, 'utf8');

        try {
            const data = JSON.parse(fileContents);
            tests.push(data);
        } catch (err) {
            console.error(err);
        }
    });

    console.log(tests);

    total_fixtures = tests.reduce(function (tot, test) {
        // return the sum with previous value
        return tot + test.fixtures.length;

        // set initial value as 0
    }, 0);

    console.log("Total fixtures:", total_fixtures);

    // Create a keyring instance
    const keyring = new Keyring({ type: 'sr25519' });

    // Initialise the provider to connect to the local node
    const provider = new WsProvider('ws://127.0.0.1:9944');

    // Create the API and wait until ready
    const api = await ApiPromise.create({ provider });

    // Retrieve the chain & node information information via rpc calls
    const [chain, nodeName, nodeVersion] = await Promise.all([
        api.rpc.system.chain(),
        api.rpc.system.name(),
        api.rpc.system.version()
    ]);

    console.log(`You are connected to chain ${chain} using ${nodeName} v${nodeVersion}`);

    const alice = keyring.addFromUri('//Alice', { name: 'Alice default' });

    const { nonce } = await api.query.system.account(alice.address);

    await processTest(tests[0], api, alice, nonce);


    // Create a extrinsic, transferring 12345 units to Bob
    // const transfer = api.tx.gearModule.submitProgram();

    // // Sign and send the transaction using our account
    // const hash = await transfer.signAndSend(alice);

    // console.log('Transfer sent with hash', hash.toHex());

}

main().catch(console.error);