/* eslint-disable no-restricted-syntax */
/* eslint-disable no-console */
/* eslint-disable max-len */
// Required imports
const {
  ApiPromise,
  WsProvider,
} = require('@polkadot/api');
const { xxhashAsHex, blake2AsU8a } = require('@polkadot/util-crypto');

// import the test keyring (already has dev keys for Alice, Bob, Charlie, Eve & Ferdie)
const testKeyring = require('@polkadot/keyring/testing');
const fs = require('fs');

function xxKey(module, key) {
  return xxhashAsHex(module, 128) + xxhashAsHex(key, 128).slice(2);
}

async function resetStorage(api, sudoPair) {
  const keys = [];
  let hash = xxKey('GearModule', 'DequeueLimit');
  keys.push(hash);

  hash = xxKey('GearModule', 'MessageQueue');
  keys.push(hash);

  hash = xxKey('GearModule', 'MessagesProcessed');
  keys.push(hash);

  await api.tx.sudo.sudo(
    api.tx.system.killStorage(
      keys,
    ),
  ).signAndSend(sudoPair, { nonce: -1 });
  await api.tx.sudo.sudo(
    api.tx.system.killPrefix(
      'g::', 1,
    ),
  ).signAndSend(sudoPair, { nonce: -1 });
  let msgOpt = await api.rpc.state.getStorage('g::msg');
  while (!msgOpt.isNone) {
    msgOpt = await api.rpc.state.getStorage('g::msg');
  }
  return msgOpt;
}

function generateProgramId(api, path, salt) {
  const binary = fs.readFileSync(path);

  const code = api.createType('Bytes', Array.from(binary));
  const codeArr = api.createType('Vec<u8>', code).toU8a();
  const saltArr = api.createType('Vec<u8>', salt).toU8a();

  const id = new Uint8Array(codeArr.length + saltArr.length);
  id.set(codeArr);
  id.set(saltArr, codeArr.length);

  return api.createType('H256', blake2AsU8a(id, 256)).toHex();
}

async function checkMessages(api, exp, programs) {
  const errors = [];
  let msgOpt = await api.rpc.state.getStorage('g::msg');
  while (msgOpt.isNone) {
    msgOpt = await api.rpc.state.getStorage('g::msg');
  }
  const messageQueue = api.createType('Vec<Message>', msgOpt.unwrap());
  if (exp.messages.length !== messageQueue.length) {
    errors.push('Messages count does not match');
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
        if (gearMemory[index] !== bytes[index - at]) {
          errors.push("Memory doesn't match");
        }
      }
    }
  }
  return errors;
}

function submitProgram(api, sudoPair, program, salt, programs) {
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
  return api.tx.gearModule.submitProgram(api.createType('Bytes', Array.from(binary)), salt, initMessage, 18446744073709551615n, 0);
}

async function processExpected(api, sudoPair, fixture, programs) {
  const output = [];
  const errors = [];

  for (let expIdx = 0; expIdx < fixture.expected.length; expIdx++) {
    const exp = fixture.expected[expIdx];
    if ('step' in exp) {
      let messagesProcessed = await api.query.gearModule.messagesProcessed();
      let deqLimit = await api.query.gearModule.dequeueLimit();
      while (deqLimit.isNone) {
        deqLimit = await api.query.gearModule.dequeueLimit();
      }
      if (deqLimit.unwrap().toNumber() !== exp.step) {
        const tx = [];

        // Set DequeueLimit
        const hash = xxKey('GearModule', 'DequeueLimit');

        tx.push(api.tx.sudo.sudo(
          api.tx.system.setStorage([[hash, api.createType('Option<u32>', api.createType('u32', exp.step)).toHex()]]),
        ));

        await api.tx.utility.batch(tx).signAndSend(sudoPair, { nonce: -1 });

        messagesProcessed = await api.query.gearModule.messagesProcessed();

        while (messagesProcessed.unwrap().toNumber() < exp.step) {
          messagesProcessed = await api.query.gearModule.messagesProcessed();
        }
      }

      if ('messages' in exp) {
        const res = await checkMessages(api, exp, programs);
        if (res.length === 0) {
          output.push('MSG: OK');
        } else {
          errors.push(`MSG ERR: ${res}`);
        }
      }

      if ('memory' in exp) {
        const res = await checkMemory(api, exp);
        if (res.length === 0) {
          output.push('MEMORY: OK');
        } else {
          errors.push(`MEMORY ERR: ${res}`);
        }
      }
    }
    // TODO: FIX IF NO STEPS
  }
  if (errors.length > 0) {
    console.log(`Fixture ${fixture.title}`);
    for (const err of errors) {
      console.log(err);
    }
    process.exit(1);
  }
  return output;
}

async function processFixture(api, sudoPair, fixture, programs) {
  const txs = [];

  if ('step' in fixture.expected[0]) {
    // Set DequeueLimit
    const hash = xxKey('GearModule', 'DequeueLimit');
    await api.tx.sudo.sudo(
      api.tx.system.setStorage([[hash, api.createType('Option<u32>', api.createType('u32', fixture.expected[0].step)).toHex()]]),
    ).signAndSend(sudoPair, { nonce: -1 });
  }

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

  await api.tx.utility.batch(txs).signAndSend(sudoPair, { nonce: -1 });

  return processExpected(api, sudoPair, fixture, programs);
}

async function processTest(test, api, sudoPair) {
  const programs = [];
  const salts = [];
  const txs = [];
  // Submit programs
  for (const fixture of test.fixtures) {
    await resetStorage(api, sudoPair);
    for (const program of test.programs) {
      const salt = Math.random().toString(36).substring(7);
      programs[program.id] = generateProgramId(api, program.path, salt);
      salts[program.id] = salt;
    }
    for (const program of test.programs) {
      const submit = submitProgram(api, sudoPair, program, salts[program.id], programs);
      txs.push(submit);
    }

    await api.tx.utility.batch(txs).signAndSend(sudoPair, { nonce: -1 });

    const out = await processFixture(api, sudoPair, fixture, programs);
    console.log(`Fixture ${fixture.title}`);
    for (const res of out) {
      console.log(res);
    }
  }
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

  for (const test of tests) {
    await processTest(test, api, adminPair);
  }
  process.exit(0);
}

main().catch(console.error);
