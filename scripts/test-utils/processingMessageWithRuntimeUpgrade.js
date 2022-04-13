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

const getDispatchMessageEnqueuedBlock = (api, { events, status }) => {
  let blockHash = undefined;
  events.forEach(({ event }) => {
    if (api.events.gear.DispatchMessageEnqueued.is(event)) {
      blockHash = status.asInBlock.toHex();
    }
  });
  return blockHash;
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

  // const messages = new Array(54).fill(api.tx.gear.sendMessage(programId, 'PING', 100_000_000, 0));
  const message = api.tx.gear.sendMessage(programId, 'PING', 100_000_000, 0);

  let codeUpdatedBlock = undefined;
  let messages = [undefined, undefined];
  await new Promise((resolve) => {
    api.tx.utility.batchAll([setCodeUnchekedWeight, message]).signAndSend(account, (events) => {
      if (events.status.isInBlock) {
        messages[0] = getDispatchMessageEnqueuedBlock(api, events);
        events.events.forEach(({ event }) => {
          if (api.events.system.CodeUpdated.is(event)) {
            codeUpdatedBlock = events.status.asInBlock.toHex();
          }
        });
        message.signAndSend(account, (secondTxEvents) => {
          if (secondTxEvents.status.isInBlock) {
            messages[1] = getDispatchMessageEnqueuedBlock(api, secondTxEvents);
            resolve();
          }
        });
      }
    });
  });

  assert.notEqual(messages[0], messages[1], 'both sendMessage txs were processed in the same block');
  assert.notStrictEqual(codeUpdatedBlock, undefined, 'setCode was not processed successfully');
  assert.notEqual(
    await messageDequeuedIsOccured(api, await getNextBlock(api, codeUpdatedBlock)),
    true,
    'A message was processed in the next block after CodeUpdated event',
  );
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
    exec("kill -9 $(pgrep -a gear-node)", (err, stdout, stderr) => {
      if (err) {
        console.log(`Unable to execute kill command`);
      }

      if (exitCode == 0) {
        console.log('✅ Test passed');
      } else {
        console.log('❌ Test failed');
      }

      process.exit(exitCode);
    });
  });
