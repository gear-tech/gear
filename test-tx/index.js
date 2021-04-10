// Required imports
const {
    ApiPromise,
    WsProvider,
    Keyring
} = require('@polkadot/api');

// import the test keyring (already has dev keys for Alice, Bob, Charlie, Eve & Ferdie)
const testKeyring = require('@polkadot/keyring/testing');
const fs = require('fs');

function submitProgram(api, sudoPair, program) {
    let binary = fs.readFileSync(program.path);

    // var bytes = 
    // console.log(Bytes(binary));
    // console.log(bytes);
    let program_id = [];
    return api.tx.gearModule.submitProgram(api.createType('Bytes', Array.from(binary)), "PING", 1000000);
}

async function processTest(test, api, sudoPair) {
    let programs_wasm = [];
    let programs_tx = [];
    let programs = [];

    for (const program of test.programs) {

        let tx = submitProgram(api, sudoPair, program);
        programs_tx.push(tx);
    }
    let index = 0;
    const unsubscribe = await api.rpc.chain.subscribeNewHeads((header) => {
        console.log(`Chain is at block: #${header.number}`);
        const element = programs_tx[index];
        element.signAndSend(sudoPair, ({
            events = [],
            status
        }) => {
            let program_id = [];
            console.log('Transaction status:', status.type);
            if (status.isFinalized) {
                console.log('Finalized block hash', status.asFinalized.toHex());
                events.forEach(({
                    event: {
                        data,
                        method,
                        section
                    },
                    phase
                }) => {
                    if (section === 'gearModule' && method === 'NewProgram') {
                        program_id = data[0];
                        console.log('\t', phase.toString(), `: ${section}.${method}`, data.toString());
                    }
                });
                // console.log(program);

                programs[index] = program_id;
            }
        });

        if (++index === test.programs.length) {
            unsubscribe();
        }
    });
    if (index == test.programs.length) {
        for (const fixture of test.fixtures) {
            for (const message of fixture.messages) {
                api.tx.gearModule.sendMessage(programs[message.desination], message.payload.value, 100000000).signAndSend(sudoPair);
            }
        }
    }


}


async function main() {
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
    // const keyring = new Keyring({
    //     type: 'sr25519'
    // });

    // Initialise the provider to connect to the local node
    const provider = new WsProvider('ws://127.0.0.1:9944');

    // Create the API and wait until ready
    const api = await ApiPromise.create({
        provider
    });

    // Retrieve the chain & node information information via rpc calls
    const [chain, nodeName, nodeVersion] = await Promise.all([
        api.rpc.system.chain(),
        api.rpc.system.name(),
        api.rpc.system.version()
    ]);

    console.log(`You are connected to chain ${chain} using ${nodeName} v${nodeVersion}`);

    // Retrieve the upgrade key from the chain state
    const adminId = await api.query.sudo.key();

    // Find the actual keypair in the keyring (if this is a changed value, the key
    // needs to be added to the keyring before - this assumes we have defaults, i.e.
    // Alice as the key - and this already exists on the test keyring)
    const keyring = testKeyring.createTestKeyring();
    const adminPair = keyring.getPair(adminId.toString());


    await processTest(tests[0], api, adminPair);


    // Create a extrinsic, transferring 12345 units to Bob
    // const transfer = api.tx.gearModule.submitProgram();

    // // Sign and send the transaction using our account
    // const hash = await transfer.signAndSend(alice);

    // console.log('Transfer sent with hash', hash.toHex());

}

main().catch(console.error);