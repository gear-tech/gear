const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const { randomAsHex } = require('@polkadot/util-crypto');
const { readFileSync } = require('fs');
const assert = require('assert');
const { exec } = require('child_process');
const { messageDispatchedIsOccurred, getBlockNumber, getNextBlock, checkProcessed, listenToUserMessageSent } = require('./util.js');

async function main(pathToDemoLoop) {
  const provider = new WsProvider('ws://127.0.0.1:9944');
  const api = await ApiPromise.create({ provider });

  // Create alice account
  const account = new Keyring({ type: 'sr25519' }).addFromUri('//Alice');

  const code = readFileSync(pathToDemoLoop);
  const codeBytes = api.createType('Bytes', Array.from(code));
  const program = api.tx.gear.uploadProgram(codeBytes, randomAsHex(20), '0x', 100_000_000_000, 0);

  const gotProcessed = checkProcessed(api);
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
  await gotProcessed(messageId, true);

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
  assert.notStrictEqual(message.exitCode, 0, `Message processed successfully`);
  assert.strictEqual(message.payload, 'Execution error: Not enough gas to continue execution', `Payload is wrong`);
  const usmBlockNumber = await getBlockNumber(api, message.blockHash);
  assert.equal(
    usmBlockNumber,
    meBlockNumber,
    `UserMessageSent and MessageEnqueued occurred in the different blocks ${usmBlockNumber}, ${meBlockNumber}`,
  );
  const nextBlock = await getNextBlock(api, usmBlockNumber);
  assert.strictEqual(
    await messageDispatchedIsOccurred(api, nextBlock),
    false,
    `Some messages were processed in the next block`,
  );
}

const pathToDemoLoop = process.argv[2];
assert.notStrictEqual(pathToDemoLoop, undefined, `Path to demo loop is not specified`);

let exitCode = undefined;

main(pathToDemoLoop)
  .then(() => {
    exitCode = 0;
  })
  .catch((error) => {
    console.error(`JS_TEST: ${error}`);
    exitCode = 1;
  })
  .finally(() => {
    exec('kill -9 $(pgrep -a gear-node)', (err, stdout, stderr) => {
      if (err) {
        console.log(`JS_TEST: Unable to execute kill command (${err})`);
        exitCode = 2;
      }

      if (exitCode == 0) {
        console.log('JS_TEST: ✅ Test passed');
      } else {
        console.log('JS_TEST: ❌ Test failed');
      }

      process.exit(exitCode);
    });
  });
