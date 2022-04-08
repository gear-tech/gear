const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const { randomAsHex } = require('@polkadot/util-crypto');
const { readFileSync } = require('fs');
const assert = require('assert/strict');

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

const checkInit = (api) => {
  const success = [];
  api.query.system.events((events) => {
    events
      .filter(({ event }) => api.events.gear.InitSuccess.is(event))
      .forEach(({ event: { method, data } }) => {
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

const main = async (pathToRuntimeCode, pathToDemoPing) => {
  // Create connection
  const provider = new WsProvider('ws://127.0.0.1:9944');
  const api = await ApiPromise.create({ provider });

  // Create alice account
  const account = new Keyring({ type: 'sr25519' }).addFromUri('//Alice');
  // Check that it is root
  assert.ok((await api.query.sudo.key()).eq(account.addressRaw));

  const isInitialized = checkInit(api);
  // Upload demo_ping
  const programId = await upload_program(api, account, pathToDemoPing);
  // Check that demo_ping was initialized
  isInitialized(programId);

  // Take runtime code
  const code = readFileSync(pathToRuntimeCode);
  const setCode = api.tx.system.setCode(api.createType('Bytes', Array.from(code)));
  const setCodeUnchekedWeight = api.tx.sudo.sudoUncheckedWeight(setCode, 1);

  const message = api.tx.gear.sendMessage(programId, 'PING', 100_000_000, 0);

  const processingBlocks = { CodeUpdated: undefined, DispatchMessageEnqueued: undefined };

  await new Promise((resolve, reject) => {
    api.tx.utility.batchAll([setCodeUnchekedWeight, message]).signAndSend(account, ({ events, status }) => {
      if (status.isInBlock) {
        events.forEach(({ event }) => {
          if (api.events.system.CodeUpdated.is(event)) {
            processingBlocks.CodeUpdated = status.asInBlock.toHex();
          } else if (api.events.gear.DispatchMessageEnqueued.is(event)) {
            processingBlocks.DispatchMessageEnqueued = status.asInBlock.toHex();
          } else if (api.events.system.ExtrinsicSuccess.is(event)) {
            resolve('ExtrinsicSuccess');
          } else if (api.events.system.ExtrinsicFailed.is(event)) {
            reject('ExtrinsicFailed');
          }
        });
      }
    });
  });
  assert.notStrictEqual(processingBlocks.CodeUpdated, undefined, 'setCode was not proccessed successfully');
  assert.notStrictEqual(
    processingBlocks.DispatchMessageEnqueued,
    undefined,
    'sendMessage was not proccessed successfully',
  );
  assert.notEqual(
    processingBlocks.CodeUpdated,
    processingBlocks.DispatchMessageEnqueued,
    'setCode and sendMessage were proccessed in the same block',
  );
  console.log('Passed');
};

const args = process.argv.slice(2);
const pathToRuntimeCode = args[0];
const pathToDemoPing = args[1];

main(pathToRuntimeCode, pathToDemoPing)
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
