/* eslint-disable no-restricted-syntax */
/* eslint-disable no-console */
/* eslint-disable max-len */
// Required imports
const {
  ApiPromise,
  WsProvider,
} = require('@polkadot/api');
const { xxhashAsHex, blake2AsHex, blake2AsU8a } = require('@polkadot/util-crypto');

// import the test keyring (already has dev keys for Alice, Bob, Charlie, Eve & Ferdie)
const testKeyring = require('@polkadot/keyring/testing');
const fs = require('fs');

function generateProgramId(api, path, salt) {
  const binary = fs.readFileSync(path);

  const code = api.createType('Bytes', Array.from(binary));
  const codeArr = api.createType('Vec<u8>', code).toU8a();
  const saltArr = api.createType('Vec<u8>', salt).toU8a();
  // codeArr = Uint8Array.from(binary);
  const id = new Uint8Array(codeArr.length + saltArr.length);
  id.set(codeArr);
  id.set(saltArr, codeArr.length);

  console.log(blake2AsHex(id, 256));

  console.log(api.createType('H256', blake2AsU8a(id, 256)).toHex());
  return api.createType('H256', blake2AsU8a(id, 256)).toHex();
}

async function checkMessages(api, exp, programs) {
  const errors = [];
  const msgOpt = await api.rpc.state.getStorage('g::msg');
  const messageQueue = api.createType('Vec<Message>', msgOpt.unwrap());
  if (exp.messages.length !== messageQueue.length) {
    errors.push("MESSAGES COUNT DOUESN'T MATCH");
    return errors;
  }

  for (let index = 0; index < exp.messages.length; index++) {
    const expMessage = exp.messages[index];
    let payload = [];
    if (expMessage.payload.kind === 'bytes') {
      payload = api.createType('Bytes', expMessage.payload.value);
    } else if (expMessage.payload.kind === 'i32') {
      payload = api.createType('Bytes', Array.from(api.createType('i32', expMessage.payload.value).toU8a()));
    } else if (expMessage.payload.kind === 'i64') {
      payload = api.createType('Bytes', Array.from(api.createType('i64', expMessage.payload.value).toU8a()));
    } else if (expMessage.payload.kind === 'f32') {
      payload = api.createType('Bytes', Array.from(api.createType('f32', expMessage.payload.value).toU8a()));
    } else if (expMessage.payload.kind === 'f64') {
      payload = api.createType('Bytes', Array.from(api.createType('f64', expMessage.payload.value).toU8a()));
    } else if (expMessage.payload.kind === 'utf-8') {
      payload = api.createType('Bytes', Array.from(api.createType('f64', expMessage.payload.value).toU8a()));
    }

    if (!messageQueue[index].payload.eq(payload)) {
      errors.push("Message payload doesn't match");
    }
    if (!messageQueue[index].dest.eq(programs[expMessage.destination])) {
      errors.push("Message destination doesn't match");
    }
    if ('gas_limit' in expMessage) {
      if (!messageQueue[index].gas_limit.toNumber().eq(expMessage.gas_limit)) {
        errors.push("Message gas_limit doesn't match");
      }
    }
  }

  return errors;
}

async function checkMemory(api, exp) {
  const errors = [];
  for (const mem of exp.memory) {
    if (mem.kind === 'shared') {
      const gearMemoryOpt = await api.rpc.state.getStorage('g::memory');
      const gearMemory = gearMemoryOpt.unwrap().toU8a();
      const at = parseInt(mem.at, 16) - (256 * 65536);
      const bytes = Uint8Array.from(Buffer.from(mem.bytes.slice(2), 'hex'));
      for (let index = at; index < at + bytes.length; index++) {
        if (gearMemory[index] != bytes[index - at]) {
          errors.push("Memory doesn't match");
        }
      }
    }
  }
  return errors;
}

function submitProgram(api, sudoPair, program, programs) {
  const binary = fs.readFileSync(program.path);

  let initMessage = [];
  if (program.init_message !== undefined) {
    if (program.init_message.kind === 'bytes') {
      initMessage = api.createType('Bytes', program.init_message.value);
    } else if (program.init_message.kind === 'i32') {
      initMessage = api.createType('Bytes', Array.from(api.createType('i32', program.init_message.value).toU8a()));
    } else if (program.init_message.kind === 'i64') {
      initMessage = api.createType('Bytes', Array.from(api.createType('i64', program.init_message.value).toU8a()));
    } else if (program.init_message.kind === 'f32') {
      initMessage = api.createType('Bytes', Array.from(api.createType('f32', program.init_message.value).toU8a()));
    } else if (program.init_message.kind === 'f64') {
      initMessage = api.createType('Bytes', Array.from(api.createType('f64', program.init_message.value).toU8a()));
    } else if (program.init_message.kind === 'utf-8') {
      if (program.init_message.value.search(/{([0-9]*)\}/) !== -1) {
        const res = program.init_message.value.match(/{([0-9]*)\}/);
        const id = Number(res[1]);
        if (programs[id] !== undefined) {
          program.init_message.value = program.init_message.value.replace(res[0], programs[id].toString().slice(2));
        }
      }
      initMessage = program.init_message.value;
    } else {
      initMessage = program.init_message.value;
    }
  }
  return api.tx.gearModule.submitProgram(api.createType('Bytes', Array.from(binary)), program.path, initMessage, 18446744073709551615n, 0);
}

async function processExpected(api, sudoPair, fixture, programs) {
  const output = [];
  for (const exp of fixture.expected) {
    if ('step' in exp) {
      let messagesProcessed = await api.query.gearModule.messagesProcessed();
      const deqLimit = await api.query.gearModule.dequeueLimit();
      if (deqLimit.unwrap().toNumber() !== exp.step) {
        const tx = [];
        // Set MessagesProcessed to zero
        // let hash = xxhashAsHex('GearModule', 128) + xxhashAsHex('MessagesProcessed', 128).slice(2);
        // tx.push(api.tx.sudo.sudo(
        //     api.tx.system.killStorage([[hash]])
        // ));

        // Set DequeueLimit
        const hash = xxhashAsHex('GearModule', 128) + xxhashAsHex('DequeueLimit', 128).slice(2);

        tx.push(api.tx.sudo.sudo(
          api.tx.system.setStorage([[hash, api.createType('Option<u32>', api.createType('u32', exp.step)).toHex()]]),
        ));

        const unsub = await api.tx.utility.batch(tx)
          .signAndSend(sudoPair, ({
            status,
          }) => {
            if (status.isFinalized) {
              unsub();
            }
          });

        messagesProcessed = await api.query.gearModule.messagesProcessed();

        while (messagesProcessed.unwrap().toNumber() < exp.step) {
          messagesProcessed = await api.query.gearModule.messagesProcessed();
        }
      }
      console.log(`done step - ${exp.step}`);

      if ('memory' in exp) {
        const res = await checkMemory(api, exp);
        if (res.length === 0) {
          output.push('MEMORY: OK');
        } else {
          output.push(`MEMORY ERR: ${res}`);
        }
      }

      if ('messages' in exp) {
        const res = await checkMessages(api, exp, programs);
        if (res.length === 0) {
          output.push('MSG: OK');
        } else {
          output.push(`MSG ERR: ${res}`);
        }
      }
    } else {
      // Remove DequeueLimit
      const hash = xxhashAsHex('GearModule', 128) + xxhashAsHex('DequeueLimit', 128).slice(2);
      api.tx.sudo.sudo(
        api.tx.system.killStorage([hash]),
      ).signAndSend(sudoPair);

      api.query.system.events(async (events) => {
        // Loop through the Vec<EventRecord>
        events.forEach(async (record) => {
          // Extract the phase, event and the event types
          const { event } = record;
          if (event.section === 'gearModule' && event.method === 'MessagesDequeued') {
            if (event.data[0].toNumber() === 0) {
              console.log('all done');

              if ('memory' in exp) {
                const res = await checkMemory(api, exp);
                if (res.length === 0) {
                  output.push('MEMORY: OK');
                } else {
                  output.push(`MEMORY ERR: ${res}`);
                }
              }

              if ('messages' in exp) {
                const res = await checkMessages(api, exp, programs);
                if (res.length === 0) {
                  output.push('MSG: OK');
                } else {
                  output.push(`MSG ERR: ${res}`);
                }
              }
            }
          }
        });
      });
    }
  }
  return output;
}

async function processFixture(api, sudoPair, fixture, programs) {
  console.log('SUBMIT MESSAGES');
  const txs = [];

  // Set MessagesProcessed to zero
  // let hash = xxhashAsHex('GearModule', 128) + xxhashAsHex('MessagesProcessed', 128).slice(2);
  // txs.push(api.tx.sudo.sudo(
  //     api.tx.system.setStorage([[hash, api.createType('Option<u32>', api.createType('u32', 0)).toHex()]])
  // ));

  // Send messages
  for (let index = 0; index < fixture.messages.length; index++) {
    const message = fixture.messages[index];
    let msg = [];
    if (message.payload.kind === 'bytes') {
      msg = api.createType('Bytes', message.payload.value);
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
        const res = message.payload.value.match(/{([0-9]*)\}/);
        const id = Number(res[1]);
        if (programs[id] !== undefined) {
          message.payload.value = message.payload.value.replace(res[0], programs[id].toString().slice(2));
        }
      }
      msg = message.payload.value;
    } else {
      msg = message.payload.value;
    }
    txs.push(api.tx.gearModule.sendMessage(programs[message.destination], msg, 18446744073709551615n, 0));
  }

  if ('step' in fixture.expected[0]) {
    // Set DequeueLimit
    const hash = xxhashAsHex('GearModule', 128) + xxhashAsHex('DequeueLimit', 128).slice(2);
    txs.push(api.tx.sudo.sudo(
      api.tx.system.setStorage([[hash, api.createType('Option<u32>', api.createType('u32', fixture.expected[0].step)).toHex()]]),
    ));
    console.log('steps = ', fixture.expected[0].step);
  }
  let out = [];

  const unsub = await api.tx.utility.batch(txs)
    .signAndSend(sudoPair, { nonce: -1 }, async ({
      status,
    }) => {
      if (status.isFinalized) {
        out = await processExpected(api, sudoPair, fixture, programs);
        unsub();
        console.log(fixture.title);
        console.log(out);
      }
    });
}

async function processTest(test, api, sudoPair) {
  let programs = [];
  let index = 0;
  let txs = [];
  // Submit programs
  for (const program of test.programs) {
    programs[program.id] = generateProgramId(api, program.path, program.path);
    const submit = submitProgram(api, sudoPair, program, programs);
    await submit.signAndSend(sudoPair, { nonce: -1 });
  }

  processFixture(api, sudoPair, test.fixtures[0], programs);
}

async function main() {
  const tests = [];

  // Load json files
  process.argv.slice(2).forEach((path) => {
    const fileContents = fs.readFileSync(path, 'utf8');

    try {
      const data = JSON.parse(fileContents);
      tests.push(data);
    } catch (err) {
      console.error(err);
    }
  });

  const totalFixtures = tests.reduce((tot, test) => tot + test.fixtures.length, 0);

  console.log('Total fixtures:', totalFixtures);

  // Create a keyring instance
  // const keyring = new Keyring({
  //     type: 'sr25519'
  // });

  // Initialise the provider to connect to the local node
  const provider = new WsProvider('ws://127.0.0.1:9944');

  // Create the API and wait until ready
  const api = await ApiPromise.create({
    provider,
    types: {
      Message: {
        source: 'H256',
        dest: 'H256',
        payload: 'Vec<u8>',
        gas_limit: 'Option<u64>',
        value: 'u128',
      },
    },
  });

  // Retrieve the chain & node information information via rpc calls
  const [chain, nodeName, nodeVersion] = await Promise.all([
    api.rpc.system.chain(),
    api.rpc.system.name(),
    api.rpc.system.version(),
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
}

main().catch(console.error);
