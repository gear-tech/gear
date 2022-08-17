const { ApiPromise, WsProvider, Keyring } = require("@polkadot/api");
const { randomAsHex } = require("@polkadot/util-crypto");
const { readFileSync } = require("fs");
const assert = require("assert/strict");
const { exec } = require("child_process");

const upload_program = (api, account, pathToDemoPing) => {
  const code = readFileSync(pathToDemoPing);
  const codeBytes = api.createType("Bytes", Array.from(code));
  const program = api.tx.gear.uploadProgram(
    codeBytes,
    randomAsHex(20),
    "0x00",
    100_000_000_000,
    0
  );
  return new Promise((resolve, reject) => {
    program.signAndSend(account, ({ events, status }) => {
      events.forEach(({ event: { method, data } }) => {
        if (method === "ExtrinsicFailed") {
          reject("SubmitProgram extrinsic failed");
        } else if (method === "MessageEnqueued" && status.isFinalized) {
          resolve(data[2].toHex());
        }
      });
    });
  });
};

const getMessageEnqueuedBlock = (api, { events, status }) => {
  let blockHash = undefined;
  events.forEach(({ event }) => {
    if (api.events.gear.MessageEnqueued.is(event)) {
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

const listenToProgramChanged = async (api) => {
  const success = [];
  const unsubscribe = await api.query.system.events((events) => {
    events
      .filter(({ event }) => api.events.gear.ProgramChanged.is(event))
      .forEach(({ event: { data } }) => {
        if (data[1].isActive) {
          success.push(data[0].toHex());
        }
      });
  });
  return (programId) => {
    unsubscribe();
    if (success.includes(programId)) {
      return true;
    } else {
      throw new Error("JS_TEST: Program initialization failed");
    }
  };
};

const messageDispatchedIsOccurred = async (api, hash) => {
  const apiAt = await api.at(hash);
  const events = await apiAt.query.system.events();
  console.log(`JS_TEST: blockHash next for block with setCode ${hash}`);
  return new Promise((resolve) => {
    if (
      events.filter(({ event }) => api.events.gear.MessagesDispatched.is(event))
        .length > 0
    ) {
      console.log(`JS_TEST: messagesDispatched occured`);
      resolve(true);
    } else {
      console.log(`JS_TEST: messagesDispatched didn't occur`);
      resolve(false);
    }
  });
};

const main = async (pathToRuntimeCode, pathToDemoPing) => {
  // Create connection
  const provider = new WsProvider("ws://127.0.0.1:9944");
  const api = await ApiPromise.create({ provider });

  // Create alice account
  const account = new Keyring({ type: "sr25519" }).addFromUri("//Alice");
  // Check that it is root
  assert.ok((await api.query.sudo.key()).eq(account.addressRaw));

  const isInitialized = await listenToProgramChanged(api);
  // Upload demo_ping
  const programId = await upload_program(api, account, pathToDemoPing);
  // Check that demo_ping was initialized
  isInitialized(programId);

  // Take runtime code
  const code = readFileSync(pathToRuntimeCode);
  const setCode = api.tx.system.setCode(
    api.createType("Bytes", Array.from(code))
  );
  const setCodeUncheckedWeight = api.tx.sudo.sudoUncheckedWeight(setCode, 0);

  // const messages = new Array(54).fill(api.tx.gear.sendMessage(programId, 'PING', 100_000_000, 0));
  const message = api.tx.gear.sendMessage(
    programId,
    "PING",
    200_000_000_000,
    0
  );

  let codeUpdatedBlock = undefined;
  let messages = [undefined, undefined];
  await new Promise((resolve) => {
    api.tx.utility
      .batchAll([setCodeUncheckedWeight, message])
      .signAndSend(account, (events) => {
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

  assert.notEqual(
    messages[0],
    messages[1],
    "JS_TEST: both sendMessage txs were processed in the same block"
  );
  console.log(`JS_TEST: message[0]: ${message[0]}, message[1]: ${message[1]}`);
  console.log(`JS_TEST: 1st assert passed`);
  assert.notStrictEqual(
    codeUpdatedBlock,
    undefined,
    "JS_TEST: setCode was not processed successfully"
  );
  console.log(`JS_TEST: 2nd assert passed`);
  assert.notEqual(
    await messageDispatchedIsOccurred(
      api,
      await getNextBlock(api, codeUpdatedBlock)
    ),
    true,
    "JS_TEST: A message was processed in the next block after CodeUpdated event"
  );
  console.log(`JS_TEST: 3rd assert passed`);
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
    console.error(`JS_TEST: ${error}`);
    exitCode = 1;
  })
  .finally(() => {
    exec("kill -9 $(pgrep -a gear-node)", (err, stdout, stderr) => {
      if (err) {
        console.log(`JS_TEST: Unable to execute kill command`);
      }

      if (exitCode == 0) {
        console.log("JS_TEST: ✅ Test passed");
      } else {
        console.log("JS_TEST: ❌ Test failed");
      }

      process.exit(exitCode);
    });
  });
