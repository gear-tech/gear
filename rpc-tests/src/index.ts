import {
  CreateType,
  DebugMode,
  GearApi,
  GearKeyring,
  ProgramDetails,
  GearMailbox,
  getWasmMetadata,
  DebugData,
} from '@gear-js/api';
import { xxhashAsHex, randomAsHex } from '@polkadot/util-crypto';
import { Option } from '@polkadot/types';
import { H256 } from '@polkadot/types/interfaces';
import { Codec } from '@polkadot/types/types';
import YAML from 'yaml';
import * as fs from 'fs';
import { KeyringPair } from '@polkadot/keyring/types';
import { IExpected, IExpMessage, IFixtures, ITestData, ITestMetadata, ITestPrograms } from './interfaces';

var metadata: ITestMetadata = {};
var programs: ITestPrograms = {};

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function xxKey(module: string, key: string) {
  return xxhashAsHex(module, 128) + xxhashAsHex(key, 128).slice(2);
}

function replaceRegex(input: string) {
  input = String(input);
  let ids = [];
  if (input.search(/\{([0-9]+)\}/g) !== -1) {
    const res = input.match(/\{([0-9]+)\}/g);
    for (const match of res) {
      const id = Number(match.slice(1, match.length - 1));
      ids.push(id);

      if (programs[id] !== undefined) {
        input = input.replace(match, programs[id].toString().slice(2));
      } else {
        // let n = new Array(64).fill(0);
        // n[1] = id;
        // input = n.join('')
      }
    }
  }
  console.log('input = ', input);
  return { input, ids };
}

function encodePayload(api: GearApi, expMessage: IExpMessage, source: string | H256) {
  let payload: Codec;
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
    payload = api.createType('Bytes', replaceRegex(expMessage.payload.value));
  } else if (expMessage.payload.kind === 'custom') {
    expMessage.payload.value = JSON.stringify(expMessage.payload.value);
    expMessage.payload.value = replaceRegex(expMessage.payload.value);
    let pid = Object.keys(programs).find((key) => programs[key] === source);
    try {
      if (expMessage.init) {
        payload = CreateType.create(metadata[pid].init_output, expMessage.payload.value, metadata[pid]);
      } else {
        payload = CreateType.create(metadata[pid].handle_output, expMessage.payload.value, metadata[pid]);
      }
    } catch (error) {
      console.log(error);
      return null;
    }
  }
  return payload;
}

function findMessage(api: GearApi, expMessage: IExpMessage, snapshots: DebugData[], start: number) {
  for (let index = start; index < snapshots.length; index++) {
    const snapshot = snapshots[index];

    if (snapshot.dispatchQueue) {
      for (let { message } of snapshot.dispatchQueue) {
        if (message.dest.eq(programs[expMessage.destination])) {
          let match = true;

          if (expMessage.payload) {
            let payload = encodePayload(api, expMessage, message.source);

            if (payload && !payload.eq(message.payload)) {
              match = false;
            }
          }

          if (expMessage.value) {
            if (!expMessage.value.eq(message.value)) {
              match = false;
            }
          }

          if (match) {
            return index;
          }
        }
      }
    }
  }
  return -1;
}

async function resetStorage(api: GearApi, sudoPair: KeyringPair) {
  const keys = [];
  const txs = [];

  let hash = xxKey('Gear', 'MessageQueue');
  keys.push(hash);

  hash =
    xxKey('Gear', 'Mailbox') +
    'de1e86a9a8c739864cf3cc5ec2bea59fd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d';
  keys.push(hash);

  txs.push(api.tx.sudo.sudo(api.tx.system.killStorage(keys)));
  txs.push(api.tx.sudo.sudo(api.tx.system.killPrefix('g::', 1)));

  await api.tx.utility.batch(txs).signAndSend(sudoPair, { nonce: -1 });
  let head = (await api.rpc.state.getStorage('g::msg::head')) as Option<Codec>;
  while (head.isSome) {
    head = (await api.rpc.state.getStorage('g::msg::head')) as Option<Codec>;
  }
}

async function checkLog(api: GearApi, exp: IExpected) {
  const errors = [];

  let mailbox = new GearMailbox(api);
  // use account id
  let messagesOpt = await mailbox.read('5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY');
  if (messagesOpt.isSome) {
    let messages = messagesOpt.unwrap();

    for (const log of exp.log) {
      if ('payload' in log) {
        let found = false;
        for (const index of Object.keys(metadata)) {
          let encoded = encodePayload(api, log, programs[index]);

          if (!encoded) {
            console.log('Skip: Cannot construct unknown type');
            found = true;
            continue;
          }

          messages.forEach((message, _id) => {
            if (encoded.toHex() === message.payload.toHex()) {
              found = true;
              return;
            }
          });
        }

        if (!found) {
          errors.push(`Not Found ${JSON.stringify(log)}`);
        }
      }
    }
  } else {
    errors.push('Empty');
  }

  return errors;
}

async function checkMessages(api: GearApi, exp: IExpected, snapshots: DebugData[]) {
  const errors = [];
  let found = 0;
  for (let index = 0; index < exp.messages.length; index++) {
    const expMessage = exp.messages[index];
    found = findMessage(api, expMessage, snapshots, found);
    if (found === -1) {
      errors.push(`Message not found (expected: ${JSON.stringify(expMessage, null, 2)})`);
      break;
    }
  }

  return errors;
}

async function checkMemory(api: GearApi, exp: IExpected, snapshots: DebugData[], programs: ITestPrograms) {
  const errors = [];

  for (const mem of exp.memory) {
    const gearProgramOpt = (await api.rpc.state.getStorage(
      `0x${Buffer.from('g::prog::').toString('hex')}${programs[mem.program_id].slice(2)}`,
    )) as Option<Codec>;
    const gearProgram = api.createType('Program', gearProgramOpt.unwrap()) as ProgramDetails;

    let at = parseInt(mem.at, 16);
    const bytes = Uint8Array.from(Buffer.from(mem.bytes.slice(2), 'hex'));
    const atPage = Math.floor(at / 65536);
    at -= atPage * 65536;

    let pages = [];

    for (let page of gearProgram.persistent_pages.keys()) {
      const buf = await api.rpc.state.getStorage(
        `0x${Buffer.from('g::prog::').toString('hex')}${programs[mem.program_id].slice(2)}::mem::${page.toHex()}`,
      );
      pages.push([page, buf]);
    }

    for (let [pageNumber, buf] of pages) {
      if (pageNumber == atPage) {
        for (let index = at; index < at + bytes.length; index++) {
          if (buf[index] !== bytes[index - at]) {
            errors.push("Memory doesn't match");
            break;
          }
        }
      }
    }
  }
  return errors;
}

async function processExpected(api: GearApi, sudoPair: KeyringPair, fixture: IFixtures, snapshots: DebugData[]) {
  const output = [];
  const errors = [];

  for (let expIdx = 0; expIdx < fixture.expected.length; expIdx++) {
    const exp = fixture.expected[expIdx];

    if (exp.step === 0) {
      continue;
    }

    if ('messages' in exp) {
      const res = await checkMessages(api, exp, snapshots);
      if (res.length === 0) {
        output.push('MSG: OK');
      } else {
        errors.push(`MSG ERR: ${res}`);
      }
    }

    if ('log' in exp) {
      const res = await checkLog(api, exp);
      if (res.length === 0) {
        output.push('LOG: OK');
      } else {
        errors.push(`LOG ERR: ${res}`);
      }
    }

    // TODO
    // if ('memory' in exp) {
    //   const res = await checkMemory(api, exp, snapshots, programs);
    //   if (res.length === 0) {
    //     output.push('MEMORY: OK');
    //   } else {
    //     errors.push(`MEMORY ERR: ${res}`);
    //   }
    // }
    if (errors.length > 0) {
      console.log(`Fixture ${fixture.title}`);
      for (const err of errors) {
        console.error(err);
      }
      process.exit(1);
    }
  }
  return output;
}

async function processFixture(api: GearApi, debugMode: DebugMode, sudoPair: KeyringPair, fixture: IFixtures) {
  const txs = [];
  const snapshots: DebugData[] = [];
  let res = [];

  // Send messages
  for (let index = 0; index < fixture.messages.length; index++) {
    const message = fixture.messages[index];
    if (message.source) {
      console.log(`Fixture ${fixture.title}: Skipped`);
      return [];
    }
    let gas_limit = 100000000000;
    let value = 0;

    if (message.gas_limit) {
      gas_limit = message.gas_limit;
    }

    if (message.value) {
      value = message.value;
    }
    let payload: any;

    if (message.payload.kind === 'bytes') {
      payload = api.createType('Bytes', message.payload.value);
    } else if (message.payload.kind === 'i32') {
      payload = api.createType('i32', message.payload.value).toU8a();
    } else if (message.payload.kind === 'i64') {
      payload = api.createType('i64', message.payload.value).toU8a();
    } else if (message.payload.kind === 'f32') {
      payload = api.createType('f32', message.payload.value).toU8a();
    } else if (message.payload.kind === 'f64') {
      payload = api.createType('f64', message.payload.value).toU8a();
    } else if (message.payload.kind === 'utf-8') {
      payload = replaceRegex(message.payload.value);
      payload = api.createType('Bytes', message.payload.value);
    } else if (message.payload.kind === 'custom') {
      message.payload.value = JSON.stringify(message.payload.value);
      payload = replaceRegex(message.payload.value);
      payload = message.payload.value;
    } else {
      payload = message.payload.value;
    }

    const meta = message.payload.kind === 'custom' ? metadata[message.destination] : { handle_input: 'Bytes' };

    txs.push(
      api.message.submit(
        {
          destination: programs[message.destination],
          payload: payload,
          gasLimit: gas_limit,
          value: value,
        },
        meta,
      ),
    );
  }
  let messagesProccessed = 0;
  let s_promise_resolve = () => {};
  let s_promise = new Promise<void>((resolve, reject) => {
    s_promise_resolve = resolve;
  });
  const unsub = await debugMode.snapshots(({ data }) => {
    snapshots.push(data);
  });

  let count = 0;
  let lastDequeAt = count;
  let isProcessing = false;
  const unsubscribeNewHeads = await api.rpc.chain.subscribeNewHeads((header) => {
    if (count - lastDequeAt > 1 && isProcessing) {
      s_promise_resolve();
      unsubscribeNewHeads();
    }
    count++;
  });
  const unsubMProccessed = await api.query.system.events((events) => {
    events
      .filter(({ event }) => api.events.gear.MessagesDequeued.is(event))
      .forEach(({ event }) => {
        isProcessing = true;
        lastDequeAt = count;
      });
  });
  await api.tx.utility.batch(txs).signAndSend(sudoPair, { nonce: -1 });

  await s_promise;

  while (snapshots.length < messagesProccessed) {
    await sleep(1000);
  }
  unsub();
  unsubMProccessed();

  return processExpected(api, sudoPair, fixture, snapshots);
}

async function processTest(testData: ITestData, api: GearApi, debugMode: DebugMode, sudoPair: KeyringPair) {
  fixtureLoop: for (const fixture of testData.fixtures) {
    const reset = await resetStorage(api, sudoPair);

    const salt = {};
    const txs = [];
    programs = {};
    metadata = {};
    for (const program of testData.programs) {
      salt[program.id] = randomAsHex(20);
      let bytes = api.createType('Bytes', Array.from(fs.readFileSync(program.path)));
      let metaBytes = fs.readFileSync(program.path.replace('.opt.wasm', '.meta.wasm'));
      metadata[program.id] = await getWasmMetadata(metaBytes);
    }

    let handled_nums = new Set();
    while (handled_nums.size < testData.programs.length) {
      programLoop: for (const program of testData.programs) {
        if (handled_nums.has(program.id)) {
          continue;
        }
        console.log('program number = ', program.id);
        if (typeof program.id === 'object') {
          console.log('Skipped');

          break fixtureLoop;
        }
        if (program.init_message) {
          let payload;
          const meta = program.init_message.kind === 'custom' ? metadata[program.id] : { init_input: 'Bytes' };
          if (program.init_message.kind === 'utf-8') {
            let { input, ids } = replaceRegex(program.init_message.value);
            for (const id of ids) {
              if (programs[id] == undefined) {
                continue programLoop;
              }
            }
            payload = api.createType('Bytes', input);
          } else if (program.init_message.kind === 'custom') {
            payload = JSON.stringify(program.init_message.value);
            payload = replaceRegex(payload);
          } else if (program.init_message.kind === 'bytes') {
            payload = api.createType('Bytes', program.init_message.value);
          }

          let res = api.program.submit(
            {
              code: fs.readFileSync(program.path),
              salt: salt[program.id],
              initPayload: payload,
              gasLimit: 100000000000,
              value: 0,
            },
            meta,
          );
          programs[program.id] = res.programId;
          console.log('program id = ', programs[program.id]);
        } else {
          const meta = { init_input: 'Bytes' };
          let res = api.program.submit(
            {
              code: fs.readFileSync(program.path),
              salt: salt[program.id],
              initPayload: [],
              gasLimit: 100000000000,
              value: 0,
            },
            meta,
          );
          programs[program.id] = res.programId;
          console.log('program id = ', programs[program.id]);
          // assert
        }
        handled_nums.add(program.id);
        txs.push(api.program.submitted);
      }
    }

    await api.tx.utility.batch(txs).signAndSend(sudoPair, { nonce: -1 });

    /* This is a workaround for chat demo program.
      Frankly speaking for any programs that may emit messages in their `init` method.

      Before new messages may be appended to the queue programs should be inited and
      outgoing messages should be queued if any. So here we just wait for several blocks
      before enqueuing new messages.
    */
    const countLimit = 5;
    let count = 0;
    const unsubscribe = await api.rpc.chain.subscribeNewHeads((_header) => {
      if (++count === countLimit) {
        unsubscribe();
      }
    });

    while (count < countLimit) {
      await sleep(100);
    }

    const out = await processFixture(api, debugMode, sudoPair, fixture);
    if (out.length > 0) {
      console.log(`Fixture ${fixture.title}: Ok`);
    }
  }
}

async function main() {
  const tests: ITestData[] = [];

  // Load yaml files
  process.argv.slice(2).forEach((path) => {
    const fileContents = fs.readFileSync(path, 'utf8').toString();

    try {
      const data = YAML.parse(fileContents);
      tests.push(data);
    } catch (err) {
      console.error(err);
      process.exit(1);
    }
  });

  const totalFixtures = tests.reduce((tot, test) => tot + test.fixtures.length, 0);

  console.log('Total fixtures:', totalFixtures);

  // Create the API and wait until ready
  const api = await GearApi.create();
  const rootKeys = await GearKeyring.fromSuri('//Alice', 'Alice default');

  console.log('root key addr: ', rootKeys.address);

  const debugMode = new DebugMode(api);

  debugMode.enable();
  await debugMode.enabled.signAndSend(rootKeys);

  for (const test of tests) {
    if (test.skipRpcTest) continue;
    console.log('Test:', test.title);
    await processTest(test, api, debugMode, rootKeys);
  }
}

main()
  .catch((err) => {
    console.error(err);
    process.exit(1);
  })
  .finally(() => process.exit());
