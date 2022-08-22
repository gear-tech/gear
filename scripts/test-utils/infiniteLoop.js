const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const { randomAsHex } = require('@polkadot/util-crypto');
const { readFileSync } = require('fs');
const assert = require('assert');
const { messageDispatchedIsOccurred, getBlockNumber, getNextBlock, checkInit } = require('./util.js');

function listenToUserMessageSent(api, programId) {
  let message;

  unsub = api.query.system.events((events) => {
    const blockHash = events.createdAtHash.toHex();
    events.forEach((d) => {
      const { event } = d;
      if (event.method === 'UserMessageSent') {
        if (event.data.message.source.eq(programId)) {
          const data = event.data.toHuman();
          message = {
            exitCode: Number(data.message.reply.exitCode),
            payload: data.message.payload,
            blockHash,
          };
        }
      }
    });
  });
  return () => message;
}

async function main(path) {
  const provider = new WsProvider('ws://127.0.0.1:9944');
  const api = await ApiPromise.create({ provider });

  // Create alice account
  const account = new Keyring({ type: 'sr25519' }).addFromUri('//Alice');

  const code = readFileSync(path);
  const codeBytes = api.createType('Bytes', Array.from(code));
  const program = api.tx.gear.uploadProgram(codeBytes, randomAsHex(20), '0x', 100_000_000_000, 0);

  const isInitialized = checkInit(api);
  const [programId, messageId] = await new Promise((resolve, reject) => {
    program.signAndSend(account, ({ events, status }) => {
      events.forEach(({ event: { method, data } }) => {
        if (method === 'ExtrinsicFailed') {
          reject('upload_program extrinsic failed');
        } else if (method === 'MessageEnqueued' && status.isFinalized) {
          resolve([data.destination.toHex(), data.id.toHex()]);
        }
      });
    });
  });
  await isInitialized(messageId);

  const gasLimit = api.consts.gearGas.blockGasLimit;

  const getMessage = listenToUserMessageSent(api, programId);

  const blockHash = await new Promise((resolve, reject) => {
    api.tx.gear.sendMessage(programId, 'PING', gasLimit, 0).signAndSend(account, ({ events, status }) => {
      events.forEach(({ event: { method, data } }) => {
        if (method === 'ExtrinsicFailed') {
          reject('send_message extrinsic failed');
        } else if (method === 'MessageEnqueued' && status.isFinalized) {
          resolve(status.asFinalized);
        }
      });
    });
  });
  const meBlockNumber = await getBlockNumber(api, blockHash);

  const message = getMessage();
  assert.notStrictEqual(message, undefined, `Message not found`);
  assert.notStrictEqual(message.exitCode, 0, `Message proccesed sucessfully`);
  assert.strictEqual(message.payload, 'Execution error: Not enough gas to continue execution', `Payload is wrong`);
  const usmBlockNumber = await getBlockNumber(api, message.blockHash);
  assert.equal(
    usmBlockNumber,
    meBlockNumber,
    `UserMessageSent and MessageEnqueued occured in the different blocks ${usmBlockNumber}, ${meBlockNumber}`,
  );
  const nextBlock = await getNextBlock(api, usmBlockNumber);
  assert.strictEqual(
    await messageDispatchedIsOccurred(api, nextBlock),
    false,
    `Some messages were processed in the next block`,
  );
}

const path = process.argv[2];
assert.notStrictEqual(path, undefined, `Path is not specified`);

let exitCode = undefined;

main(path)
  .then(() => {
    exitCode = 0;
  })
  .catch((error) => {
    console.error(error);
    exitCode = 1;
  })
  .finally(() => {
    process.exit(exitCode);
  });
