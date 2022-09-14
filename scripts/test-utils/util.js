const { randomAsHex } = require('@polkadot/util-crypto');
const assert = require('assert');
const { readFileSync } = require('fs');

async function messageDispatchedIsOccurred(api, hash) {
  const apiAt = await api.at(hash);
  const events = await apiAt.query.system.events();
  return new Promise((resolve) => {
    if (events.filter(({ event }) => api.events.gear.MessagesDispatched.is(event)).length > 0) {
      resolve(true);
    } else {
      resolve(false);
    }
  });
}

async function getLastBlockNumber(api) {
  const block = await api.rpc.chain.getBlock();
  return block.block.header.number.toNumber();
}

async function getBlockNumber(api, hash) {
  const block = await api.rpc.chain.getBlock(hash);
  return block.block.header.number.toNumber();
}

async function getNextBlock(api, blockNumber) {
  return api.rpc.chain.getBlockHash(blockNumber + 1);
}

function checkProcessed(api) {
  let processedMessages = new Map();

  // TODO: does not update processedMessages after first message (issue #1462)
  const unsubPromise = api.query.system.events((events) => {
    events.forEach(({ event: { method, data } }) => {
      if (method === 'MessagesDispatched') {
        for (const [id, status] of data.statuses) {
          processedMessages.set(id.toHex(), status.isSuccess);
        }
      }
    });
  });

  return (messageId, status = null) => {
    unsubPromise.then((unsub) => unsub());
    assert(processedMessages.has(messageId) === true, 'Message was not processed');
    if (status !== null) {
      if (status) {
        assert.strictEqual(processedMessages.get(messageId), true, 'Message failed');
      } else {
        assert.strictEqual(processedMessages.get(messageId), false, 'Message succeed');
      }
    }
  };
}

function getMessageEnqueuedBlock(api, { events, status }) {
  let blockHash = undefined;

  events.forEach(({ event }) => {
    if (api.events.gear.MessageEnqueued.is(event)) {
      blockHash = status.asInBlock.toHex();
    }
  });
  return blockHash;
}

function uploadProgram(api, account, pathToDemo) {
  const code = readFileSync(pathToDemo);
  const codeBytes = api.createType('Bytes', Array.from(code));
  const program = api.tx.gear.uploadProgram(codeBytes, randomAsHex(20), '0x00', 100_000_000_000, 0);
  return new Promise((resolve, reject) => {
    program.signAndSend(account, ({ events, status }) => {
      events.forEach(({ event: { method, data } }) => {
        if (method === 'ExtrinsicFailed') {
          reject('SubmitProgram extrinsic failed');
        } else if (method === 'MessageEnqueued' && status.isFinalized) {
          resolve([data.destination.toHex(), data.id.toHex()]);
        }
      });
    });
  });
}

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

module.exports = {
  messageDispatchedIsOccurred,
  getLastBlockNumber,
  getBlockNumber,
  getNextBlock,
  checkProcessed,
  getMessageEnqueuedBlock,
  uploadProgram,
  listenToUserMessageSent,
};
