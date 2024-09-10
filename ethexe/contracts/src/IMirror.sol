// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

// TODO (breathx): sort here everything.
interface IMirror {
    /* Events section */

    /**
     * @dev Emitted when the state hash of program is changed.
     *
     * NOTE: It's event for USERS: it informs about state changes.
     */
    event StateChanged(bytes32 stateHash);

    /**
     * @dev Emitted when a new message is sent to be queued.
     *
     * NOTE:    It's event for NODES:
     *  it requires to insert message in the program's queue.
     */
    event MessageQueueingRequested(bytes32 id, address indexed source, bytes payload, uint128 value);

    /**
     * @dev Emitted when a new reply is sent and requested to be verified and queued.
     *
     * NOTE:    It's event for NODES:
     *  it requires to insert message in the program's queue, if message, exists.
     */
    event ReplyQueueingRequested(bytes32 repliedTo, address indexed source, bytes payload, uint128 value);

    /**
     * @dev Emitted when a reply's value is requested to be verified and claimed.
     *
     * NOTE:    It's event for NODES:
     *  it requires to claim value from message, if exists.
     */
    event ValueClaimingRequested(bytes32 claimedId, address indexed source);

    /**
     * @dev Emitted when a user requests program's executable balance top up with his tokens.
     *
     * NOTE:    It's event for NODES:
     *  it requires to top up balance of the program.
     */
    event ExecutableBalanceTopUpRequested(uint128 value);

    /**
     * @dev Emitted when the program sends outgoing message.
     *
     * NOTE:    It's event for USERS:
     *  it informs about new message sent from program.
     */
    event Message(bytes32 id, address indexed destination, bytes payload, uint128 value);

    /**
     * @dev Emitted when the program sends reply message.
     *
     * NOTE:    It's event for USERS:
     *  it informs about new reply sent from program.
     */
    event Reply(bytes payload, uint128 value, bytes32 replyTo, bytes4 indexed replyCode);

    // TODO (breathx): should we deposit it? should we notify about successful reply sending?
    /**
     * @dev Emitted when a user succeed in claiming value request and receives balance.
     *
     * NOTE:    It's event for USERS:
     *  it informs about value claimed.
     */
    event ValueClaimed(bytes32 claimedId, uint128 value);

    /* Functions section */

    /* Operational functions */

    function stateHash() external view returns (bytes32);

    function nonce() external view returns (uint256);

    function router() external view returns (address);

    function decoder() external view returns (address);

    /* Primary Gear logic */

    function sendMessage(bytes calldata payload, uint128 value) external payable returns (bytes32);

    function sendReply(bytes32 repliedTo, bytes calldata payload, uint128 value) external payable;

    function claimValue(bytes32 claimedId) external;

    function executableBalanceTopUp(uint128 value) external payable;

    /* Router-driven state and funds management */
    // NOTE: all of these methods will have additional handler (with hooks) for decoder.

    function updateState(bytes32 newStateHash) external;

    function messageSent(bytes32 id, address destination, bytes calldata payload, uint128 value) external;

    function replySent(address destination, bytes calldata payload, uint128 value, bytes32 replyTo, bytes4 replyCode)
        external;

    function valueClaimed(bytes32 claimedId, address destination, uint128 value) external;

    function executableBalanceBurned(uint128 value) external;

    function createDecoder(address implementation, bytes32 salt) external;

    // TODO (breathx): consider removal of this in favor of separated creation and init.
    function initMessage(address source, bytes calldata payload, uint128 value, uint128 executableBalance) external;
}
