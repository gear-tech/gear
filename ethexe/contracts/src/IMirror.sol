// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Gear} from "./libraries/Gear.sol";

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
    event MessageQueueingRequested(bytes32 id, address indexed source, bytes payload, uint128 value, bool callReply);

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
     * @dev Emitted when a user requests program's owned balance top up with his Ether.
     *
     * NOTE:    It's event for NODES:
     *  it requires to top up balance of the program (in Ether).
     */
    event OwnedBalanceTopUpRequested(uint128 value);

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
     * @dev Emitted when the program fails to call outgoing message to other contracts.
     *
     * NOTE:    It's event for USERS:
     *  it informs about failed message call from program.
     */
    event MessageCallFailed(bytes32 id, address indexed destination, uint128 value);

    /**
     * @dev Emitted when the program sends reply message.
     *
     * NOTE:    It's event for USERS:
     *  it informs about new reply sent from program.
     */
    event Reply(bytes payload, uint128 value, bytes32 replyTo, bytes4 indexed replyCode);

    /**
     * @dev Emitted when the program fails to call reply message to other contracts.
     *
     * NOTE:    It's event for USERS:
     *  it informs about failed reply call from program.
     */
    event ReplyCallFailed(uint128 value, bytes32 replyTo, bytes4 indexed replyCode);

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

    function router() external view returns (address);

    function inheritor() external view returns (address);

    function initializer() external view returns (address);

    function stateHash() external view returns (bytes32);

    function nonce() external view returns (uint256);

    /* Primary Gear logic */

    function sendMessage(bytes calldata payload, bool callReply) external payable returns (bytes32);

    function sendReply(bytes32 repliedTo, bytes calldata payload) external payable;

    function claimValue(bytes32 claimedId) external;

    function executableBalanceTopUp(uint128 value) external;

    function transferLockedValueToInheritor() external;

    /* Router-driven state and funds management */

    function initialize(address initializer, address abiInterface, bool isSmall) external;

    function performStateTransition(Gear.StateTransition calldata transition) external returns (bytes32);

    function ownedBalanceTopUpFromRouter() external payable;
}
