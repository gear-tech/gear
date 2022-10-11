const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const { readFileSync } = require('fs');
const assert = require('assert/strict');
const { exec } = require('child_process');
const { messageDispatchedIsOccurred, getBlockNumber, getNextBlock, checkProcessed, getMessageEnqueuedBlock, uploadProgram } = require('./util.js');

async function main(pathToRuntimeCode, pathToDemoPing) {
  // Create connection
  const provider = new WsProvider('ws://127.0.0.1:9944');
  const api = await ApiPromise.create({ provider });

  // Create alice account
  const account = new Keyring({ type: 'sr25519' }).addFromUri('//Alice');
  // Check that it is root
  assert.ok((await api.query.sudo.key()).eq(account.addressRaw));

  const gotProcessed = checkProcessed(api);

  // Upload demo_ping
  const [programId, messageId] = await uploadProgram(api, account, pathToDemoPing);

  // Check that demo_ping was successfully initialized.
  await gotProcessed(messageId, true);

  // Take runtime code
  const code = readFileSync(pathToRuntimeCode);
  const setCode = api.tx.system.setCode(api.createType('Bytes', Array.from(code)));
  const setCodeUncheckedWeight = api.tx.sudo.sudoUncheckedWeight(setCode, 0);

  const message = api.tx.gear.sendMessage(programId, 'PING', 200_000_000_000, 0);

  let codeUpdatedBlock = undefined;
  let messages = [undefined, undefined];
  await new Promise((resolve) => {
    api.tx.utility.batchAll([setCodeUncheckedWeight, message]).signAndSend(account, (events) => {
      if (events.status.isInBlock) {
        messages[0] = getMessageEnqueuedBlock(api, events);
        events.events.forEach(({ event }) => {
          if (api.events.system.CodeUpdated.is(event)) {
            codeUpdatedBlock = events.status.asInBlock.toHex();
          }
        });
        message.signAndSend(account, (secondTxEvents) => {
          if (secondTxEvents.status.isInBlock) {
            messages[1] = getMessageEnqueuedBlock(api, secondTxEvents);
            resolve();
          }
        });
      }
    });
  });

  assert.notEqual(messages[0], messages[1], 'JS_TEST: both sendMessage txs were processed in the same block');
  console.log(`JS_TEST: message[0]: ${message[0]}, message[1]: ${message[1]}`);
  console.log(`JS_TEST: 1st assert passed`);
  assert.notStrictEqual(codeUpdatedBlock, undefined, 'JS_TEST: setCode was not processed successfully');
  console.log(`JS_TEST: 2nd assert passed`);
  assert.notEqual(
    await messageDispatchedIsOccurred(api, await getNextBlock(api, await getBlockNumber(api, codeUpdatedBlock))),
    true,
    'JS_TEST: A message was processed in the next block after CodeUpdated event',
  );
  console.log(`JS_TEST: 3rd assert passed`);
};

const args = process.argv.slice(2);
const pathToRuntimeCode = args[0];
assert.notStrictEqual(pathToRuntimeCode, undefined, `Path to runtime code is not specified`);
const pathToDemoPing = args[1];
assert.notStrictEqual(pathToDemoPing, undefined, `Path to demo ping is not specified`);
let exitCode = undefined;

main(pathToRuntimeCode, pathToDemoPing)
  .then(() => {
    exitCode = 0;
  })
  .catch((error) => {
    console.error(`JS_TEST: ${error}`);
    exitCode = 1;
  })
  .finally(() => {
    exec('pgrep -f "release/gear" | xargs kill -9', (err, stdout, stderr) => {
      if (err) {
        console.log(`JS_TEST: Unable to execute kill command (${err})`);
      }

      if (exitCode == 0) {
        console.log('JS_TEST: ✅ Test passed');
      } else {
        console.log(`JS_TEST: ❌ Test failed (${exitCode})`);
      }

      process.exit(exitCode);
    });
  });
