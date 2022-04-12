const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const { randomAsHex } = require('@polkadot/util-crypto');
const { readFileSync } = require('fs');
const assert = require('assert/strict');
const { exec } = require('child_process');

const upload_program = (api, account, pathToDemoPing) => {
  const code = readFileSync(pathToDemoPing);
  const codeBytes = api.createType('Bytes', Array.from(code));
  const program = api.tx.gear.submitProgram(codeBytes, randomAsHex(20), '0x00', 200_000_000, 0);
  return new Promise((resolve, reject) => {
    program.signAndSend(account, ({ events, status }) => {
      events.forEach(({ event: { method, data } }) => {
        if (method === 'ExtrinsicFailed') {
          reject('SubmitProgram extrinsic failed');
        } else if (method === 'InitMessageEnqueued' && status.isFinalized) {
          resolve(data[0].programId.toHex());
        }
      });
    });
  });
};

const getNextBlock = async (api, hash) => {
  const block = await api.rpc.chain.getBlock(hash);
  const blockNumber = block.block.header.number.toNumber();
  return api.rpc.chain.getBlockHash(blockNumber + 1);
};

const listenToInit = (api) => {
  const success = [];
  api.query.system.events((events) => {
    events
      .filter(({ event }) => api.events.gear.InitSuccess.is(event))
      .forEach(({ event: { data } }) => {
        success.push(data[0].programId.toHex());
      });
  });
  return (programId) => {
    if (success.includes(programId)) {
      return true;
    } else {
      throw new Error('Program initialization failed');
    }
  };
};

const messageDequeuedIsOccured = async (api, hash) => {
  const apiAt = await api.at(hash);
  const events = await apiAt.query.system.events();
  return new Promise((resolve) => {
    if (events.filter(({ event }) => api.events.gear.MessagesDequeued.is(event)).length > 0) {
      resolve(true);
    } else {
      resolve(false);
    }
  });
};

const main = async (pathToRuntimeCode, pathToDemoPing) => {
  // Create connection
  const provider = new WsProvider('ws://127.0.0.1:9944');
  const api = await ApiPromise.create({ provider });

  // Create alice account
  const account = new Keyring({ type: 'sr25519' }).addFromUri('//Alice');
  // Check that it is root
  assert.ok((await api.query.sudo.key()).eq(account.addressRaw));

  const isInitialized = listenToInit(api);
  // Upload demo_ping
  const programId = await upload_program(api, account, pathToDemoPing);
  // Check that demo_ping was initialized
  isInitialized(programId);

  // Take runtime code
  const code = readFileSync(pathToRuntimeCode);
  const setCode = api.tx.system.setCode(api.createType('Bytes', Array.from(code)));
  const setCodeUnchekedWeight = api.tx.sudo.sudoUncheckedWeight(setCode, 0);

  const messages = new Array(54).fill(api.tx.gear.sendMessage(programId, 'PING', 100_000_000, 0));

  let codeUpdatedBlock = undefined;
  let proccessedMessagesCount = undefined;
  await new Promise((resolve, reject) => {
    api.tx.utility.batchAll([setCodeUnchekedWeight, ...messages]).signAndSend(account, ({ events, status }) => {
      if (status.isFinalized) {
        proccessedMessagesCount = events.filter(({ event }) =>
          api.events.gear.DispatchMessageEnqueued.is(event),
        ).length;
        events.forEach(({ event }) => {
          if (api.events.system.CodeUpdated.is(event)) {
            codeUpdatedBlock = status.asFinalized.toHex();
          } else if (api.events.system.ExtrinsicSuccess.is(event)) {
            resolve('ExtrinsicSuccess');
          } else if (api.events.system.ExtrinsicFailed.is(event)) {
            reject('ExtrinsicFailed');
          }
        });
      }
    });
  });

  assert.notStrictEqual(proccessedMessagesCount, undefined, 'sendMessage txs were not proccessed successfully');
  assert.equal(proccessedMessagesCount, 54, 'not all sendMessage txs were proccessed successfully');
  assert.notStrictEqual(codeUpdatedBlock, undefined, 'setCode was not proccessed successfully');
  assert.notEqual(
    await messageDequeuedIsOccured(api, await getNextBlock(api, codeUpdatedBlock)),
    true,
    'setCode and sendMessage were proccessed in the same block',
  );

  console.log('Passed');
};

const args = process.argv.slice(2);
const pathToRuntimeCode = args[0];
const pathToDemoPing = args[1];
let exitCode = undefined;

main(pathToRuntimeCode, pathToDemoPing)
  .then(() => {
    exitCode = 0;
  })
  .catch((error) => {
    console.error(error);
    exitCode = 1;
  })
  .finally(() => {
    exec('echo GEAR');
    process.exit(exitCode);
  });
