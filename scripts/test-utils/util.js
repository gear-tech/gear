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

async function getBlockNumber(api, hash) {
  const block = await api.rpc.chain.getBlock(hash);
  return block.block.header.number.toNumber();
}

async function getNextBlock(api, blockNumber) {
  return api.rpc.chain.getBlockHash(blockNumber + 1);
}

function checkInit(api) {
  let unsub;
  let messages = new Map();

  unsub = api.query.system.events((events) => {
    events.forEach(({ event }) => {
      switch (event.method) {
        case 'MessagesDispatched':
          for (const [id, status] of event.data.statuses) {
            if (messages.has(id.toHex())) {
              if (status.isFailed) {
                messages.set(id.toHex(), Promise.reject(`Program initialization failed`));
                break;
              }
              if (status.isSuccess) {
                messages.set(id.toHex(), Promise.resolve());
                break;
              }
            }
          }
          break;
      }
    });
  });

  return async (messageId) => {
    (await unsub)();
    return messages.get(messageId);
  };
}

module.exports = { messageDispatchedIsOccurred, getBlockNumber, getNextBlock, checkInit };
