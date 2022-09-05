const { ApiPromise, WsProvider, Keyring } = require('@polkadot/api');
const assert = require('assert/strict');
const { exec } = require('child_process');
const { checkProcessed, uploadProgram, getLastBlockNumber } = require('./util.js');
const { setTimeout } = require("timers/promises");

async function main(demoPaths) {
    // Create connection
    const provider = new WsProvider('ws://127.0.0.1:9944');
    const api = await ApiPromise.create({ provider });

    // Create alice account
    const account = new Keyring({ type: 'sr25519' }).addFromUri('//Alice');
    // Check that it is root
    assert.ok((await api.query.sudo.key()).eq(account.addressRaw));

    const gotProcessed = checkProcessed(api);

    for (const pathToDemo of demoPaths) {
        console.log(`Uploading demo: ${pathToDemo}`);

        const [_programId, messageId] = await uploadProgram(api, account, pathToDemo);
        await gotProcessed(messageId);
    }

    const bn = await getLastBlockNumber(api);

    // Waiting for 3 seconds.
    await setTimeout(3000);

    assert.ok(bn < await getLastBlockNumber(api), 'There is no produced blocks');
  };

  const args = process.argv.slice(2);
  assert.notEqual(args.length, 0, "No demo paths passed");
  let exitCode = undefined;

  main(args)
    .then(() => {
      exitCode = 0;
    })
    .catch((error) => {
      console.error(`JS_TEST: ${error}`);
      exitCode = 1;
    })
    .finally(() => {
      exec('pgrep -f "gear-node" | xargs kill -9', (err, stdout, stderr) => {
        if (err) {
          console.log(`JS_TEST: Unable to execute kill command (${err})`);
          exitCode = 2;
        }

        if (exitCode == 0) {
          console.log('JS_TEST: ✅ Test passed');
        } else {
          console.log(`JS_TEST: ❌ Test failed (${exitCode})`);
        }

        process.exit(exitCode);
      });
    });
