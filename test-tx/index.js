// Required imports
const {
    ApiPromise,
    WsProvider,
    Keyring
} = require('@polkadot/api');
const { xxhashAsHex, xxhashAsU8a } = require('@polkadot/util-crypto');
const { u8aToHex } = require('@polkadot/util');

// import the test keyring (already has dev keys for Alice, Bob, Charlie, Eve & Ferdie)
const testKeyring = require('@polkadot/keyring/testing');
const fs = require('fs');

let p_index = 0;

function submitProgram(api, sudoPair, program, programs) {
    let binary = fs.readFileSync(program.path);

    // var bytes = 
    // console.log(Bytes(binary));
    // console.log(bytes);
    let init_message = [];
    if (program.init_message !== undefined) {
        if (program.init_message.kind === 'bytes') {
            init_message = api.createType('Bytes', Array.from(program.init_message.value.slice(2)));
        } else if (program.init_message.kind === 'i32') {
            msg = api.createType('Bytes', Array.from(api.createType('i32', message.payload.value).toU8a()));
        } else if (program.init_message.kind === 'i64') {
            msg = api.createType('Bytes', Array.from(api.createType('i64', message.payload.value).toU8a()));
        } else if (program.init_message.kind === 'f32') {
            msg = api.createType('Bytes', Array.from(api.createType('f32', message.payload.value).toU8a()));
        } else if (program.init_message.kind === 'f64') {
            msg = api.createType('Bytes', Array.from(api.createType('f64', message.payload.value).toU8a()));
        } else if (program.init_message.kind === 'utf-8') {
            if (program.init_message.value.search(/{([0-9]*)\}/) !== -1) {
                let res = program.init_message.value.match(/{([0-9]*)\}/);
                let id = Number(res[1]);
                if (programs[id] !== undefined) {
                    program.init_message.value = program.init_message.value.replace(res[0], programs[id].toString().slice(2));
                }
            }
            init_message = program.init_message.value;
        } else {
            init_message = program.init_message.value;
        }
    }
    return api.tx.gearModule.submitProgram(api.createType('Bytes', Array.from(binary)), init_message, 18446744073709551615n);
}

async function processFixture(api, sudoPair, fixture, programs) {
    let msg_index = 0;
    console.log("SUBMIT MESSAGES");
    let txs = [];
    for (let index = 0; index < fixture.messages.length; index++) {

        const message = fixture.messages[index];

        if (message.payload.kind === 'bytes') {
            msg = api.createType('Bytes', Array.from(message.payload.value.slice(2)));
        } else if (message.payload.kind === 'i32') {
            msg = api.createType('Bytes', Array.from(api.createType('i32', message.payload.value).toU8a()));
        } else if (message.payload.kind === 'i64') {
            msg = api.createType('Bytes', Array.from(api.createType('i64', message.payload.value).toU8a()));
        } else if (message.payload.kind === 'f32') {
            msg = api.createType('Bytes', Array.from(api.createType('f32', message.payload.value).toU8a()));
        } else if (message.payload.kind === 'f64') {
            msg = api.createType('Bytes', Array.from(api.createType('f64', message.payload.value).toU8a()));
        } else if (message.payload.kind === 'utf-8') {
            if (message.payload.value.search(/{([0-9]*)\}/) !== -1) {
                let res = message.payload.value.match(/{([0-9]*)\}/);
                let id = Number(res[1]);
                if (programs[id] !== undefined) {
                    message.payload.value = message.payload.value.replace(res[0], programs[id].toString().slice(2));
                }
            }
            msg = message.payload.value;
        } else {
            msg = message.payload.value;
        }
        txs.push(api.tx.gearModule.sendMessage(programs[message.destination], msg, 18446744073709551615n));
        // api.tx.gearModule.sendMessage(programs[message.destination], msg, 18446744073709551615n).signAndSend(sudoPair, ({
        //     events = [],
        //     status
        // }) => {
        //     console.log('Transaction status:', status.type);
        //     if (status.isFinalized) {
        //         console.log('Finalized block hash', status.asFinalized.toHex());
        //         events.forEach(({
        //             event: {
        //                 data,
        //                 method,
        //                 section
        //             },
        //             phase
        //         }) => {
        //             console.log('\t', phase.toString(), `: ${section}.${method}`, data.toString());
        //             // if (section === 'gearModule' && method === 'NewProgram') {
        //             //     program_id = data[0];
        //             //     console.log('\t', phase.toString(), `: ${section}.${method}`, data.toString());
        //             // }
        //         });
        //     }
        // });
    }
    api.tx.utility.batch(txs)
        .signAndSend(sudoPair, ({
            status
        }) => {
            if (status.isInBlock) {
                console.log(`included in ${status.asInBlock}`);
            }
        });
}

async function processTest(test, api, sudoPair) {
    let programs = [];
    // test.programs.reverse();
    let txs = [];
    for (let index = 0; index < test.programs.length; index++) {
        const program = test.programs[index];
        txs.push(submitProgram(api, sudoPair, program, programs));
    }
    let hash = xxhashAsHex('GearModule', 128) + xxhashAsHex('DequeueLimit', 128).slice(2);
    txs.push(api.tx.sudo.sudo(
        api.tx.system.setStorage([[hash, api.createType('Option<u32>', api.createType('u32', 2)).toHex()]])
    ))
    const unsub = await api.tx.utility.batchAll(txs)
        .signAndSend(sudoPair, ({
            events = [],
            status
        }) => {
            if (status.isInBlock) {
                events.forEach(({
                    event: {
                        data,
                        method,
                        section
                    },
                    phase
                }) => {
                    if (section === 'gearModule' && method === 'NewProgram') {
                        for (const program of test.programs) {
                            if (programs[program.id] === undefined) {
                                programs[program.id] = data[0];
                            }
                        }
                        console.log('\t', phase.toString(), `: ${section}.${method}`, data.toString());
                    }
                });
                console.log(`included in ${status.asInBlock}`);
                console.log(programs);
                processFixture(api, sudoPair, test.fixtures[0], programs);
                unsub();
            }
        });


    //     if (p_index === test.programs.length) {
    //         unsubscribe();
    //         processFixture(api, sudoPair, test.fixtures[0], programs);

    //     }
    // });
    // if (index == test.programs.length) {
    //     for (const fixture of test.fixtures) {

    //     }
    // }


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
    // ['test.json'].forEach(path => {
    //     const fileContents = fs.readFileSync(path, 'utf8');

    //     try {
    //         const data = JSON.parse(fileContents);
    //         tests.push(data);
    //     } catch (err) {
    //         console.error(err);
    //     }
    // });

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