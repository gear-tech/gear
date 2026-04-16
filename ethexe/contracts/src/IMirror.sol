// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {Gear} from "./libraries/Gear.sol";

// TODO (breathx): sort here everything.
/**
 * @dev Interface for the Mirror contract.
 */
interface IMirror {
    /* # Events */

    /**
     * @dev Emitted when the state hash of program is changed.
     * @param stateHash The new state hash of the program.
     *
     * NOTE: It's event for USERS: it informs about state changes.
     */
    event StateChanged(bytes32 stateHash);

    /**
     * @dev Emitted when a new message is sent to be queued.
     * @param id Message ID.
     * @param source Message source address.
     * @param payload Message payload.
     * @param value Message value.
     * @param callReply Indicates whether the message is sent with callReply flag.
     *
     * NOTE:    It's event for NODES:
     *  it requires to insert message in the program's queue.
     */
    event MessageQueueingRequested(bytes32 id, address indexed source, bytes payload, uint128 value, bool callReply);

    /**
     * @dev Emitted when a new reply is sent and requested to be verified and queued.
     * @param repliedTo The ID of the message being replied to.
     * @param source The address of the reply sender.
     * @param payload The payload of the reply.
     * @param value The value of the reply.
     *
     * NOTE:    It's event for NODES:
     *  it requires to insert message in the program's queue, if message, exists.
     */
    event ReplyQueueingRequested(bytes32 repliedTo, address indexed source, bytes payload, uint128 value);

    /**
     * @dev Emitted when a reply's value is requested to be verified and claimed.
     * @param claimedId The ID of the message or reply being claimed.
     * @param source The address of the claim sender.
     *
     * NOTE:    It's event for NODES:
     *  it requires to claim value from message, if exists.
     */
    event ValueClaimingRequested(bytes32 claimedId, address indexed source);

    /**
     * @dev Emitted when a user requests program's owned balance top up with his Ether.
     * @param value The amount of Ether the user wants to top up.
     *
     * NOTE:    It's event for NODES:
     *  it requires to top up balance of the program (in Ether).
     */
    event OwnedBalanceTopUpRequested(uint128 value);

    /**
     * @dev Emitted when a user requests program's executable balance top up with his tokens.
     * @param value The amount of tokens the user wants to top up.
     *
     * NOTE:    It's event for NODES:
     *  it requires to top up balance of the program.
     */
    event ExecutableBalanceTopUpRequested(uint128 value);

    /**
     * @dev Emitted when the program sends outgoing message.
     * @param id Message ID.
     * @param destination Message destination address.
     * @param payload Message payload.
     * @param value Message value.
     *
     * NOTE:    It's event for USERS:
     *  it informs about new message sent from program.
     */
    event Message(bytes32 id, address indexed destination, bytes payload, uint128 value);

    /**
     * @dev Emitted when the program fails to call outgoing message to other contracts.
     * @param id Message ID.
     * @param destination Message destination address.
     * @param value Message value.
     *
     * NOTE:    It's event for USERS:
     *  it informs about failed message call from program.
     */
    event MessageCallFailed(bytes32 id, address indexed destination, uint128 value);

    /**
     * @dev Emitted when the program sends reply message.
     * @param payload Reply message payload.
     * @param value Reply message value.
     * @param replyTo The ID of the message being replied to.
     * @param replyCode The code of the reply, which can be used to identify the type of reply.
     *
     * NOTE:    It's event for USERS:
     *  it informs about new reply sent from program.
     */
    event Reply(bytes payload, uint128 value, bytes32 replyTo, bytes4 indexed replyCode);

    /**
     * @dev Emitted when the program fails to call reply message to other contracts.
     * @param value Reply message value.
     * @param replyTo The ID of the message being replied to.
     * @param replyCode The code of the reply, which can be used to identify the type of reply.
     *
     * NOTE:    It's event for USERS:
     *  it informs about failed reply call from program.
     */
    event ReplyCallFailed(uint128 value, bytes32 replyTo, bytes4 indexed replyCode);

    // TODO (breathx): should we deposit it? should we notify about successful reply sending?
    /**
     * @dev Emitted when a user succeed in claiming value request and receives balance.
     * @param claimedId The ID of the message or reply being claimed.
     * @param value The amount of value claimed.
     *
     * NOTE:    It's event for USERS:
     *  it informs about value claimed.
     */
    event ValueClaimed(bytes32 claimedId, uint128 value);

    /**
     * @dev Emitted when the program fails to transfer locked value to inheritor after exit.
     * @param inheritor The address of the inheritor.
     * @param value The amount of locked value that failed to transfer.
     *
     * NOTE:    It's event for USERS:
     *  it informs about failed transfer of locked value to inheritor after exit.
     */
    event TransferLockedValueToInheritorFailed(address inheritor, uint128 value);

    /**
     * @dev Emitted when the program fails to transfer value to destination after failed call
     * @param destination The address of the destination.
     * @param value The amount of value that failed to transfer.
     *
     * NOTE:    It's event for USERS:
     *  it informs about failed transfer of value to destination after failed call.
     */
    event ReplyTransferFailed(address destination, uint128 value);

    /**
     * @dev Emitted when a user fails in claiming value request and doesn't receive balance.
     * @param claimedId The ID of the message or reply being claimed.
     * @param value The amount of value that failed to claim.
     *
     * NOTE:    It's event for USERS:
     *  it informs about failed value claim.
     */
    event ValueClaimFailed(bytes32 claimedId, uint128 value);

    /* # Errors */

    /**
     * @dev Thrown when the first (init) message is not created by the initializer.
     */
    error InitMessageNotCreated();

    /**
     * @dev Thrown when the first (init) message is not created and the caller is not the initializer.
     */
    error InitMessageNotCreatedAndCallerNotInitializer();

    /**
     * @dev Thrown when the program is exited and the call is attempted.
     */
    error ProgramExited();

    /**
     * @dev Thrown when the program is not exited and the call is attempted.
     */
    error ProgramNotExited();

    /**
     * @dev Thrown when the caller is not the `Router`.
     */
    error CallerNotRouter();

    /**
     * @dev The operation failed because the `Router` contract is paused and pause-protected Mirror call is attempted.
     */
    error EnforcedPause();

    /**
     * @dev Thrown when the transfer of Vara to the `Router` fails.
     */
    error WVaraTransferFailed();

    /**
     * @dev Thrown when the transfer of Ether to the `Router` fails.
     */
    error EtherTransferToRouterFailed();

    /**
     * @dev Thrown when the transfer of locked value to the inheritor fails (in external call).
     */
    error TransferLockedValueToInheritorExternalFailed();

    error InitializerAlreadySet();

    error IsSmallAlreadySet();

    error AbiInterfaceAlreadySet();

    error InvalidActorId();

    error InheritorMustBeZero();

    error InvalidFallbackCall();

    /* # Functions section */

    /* # Operational functions */

    /**
     * @dev Returns the address of the `Router` contract, which is the sole authority
     *      to modify the state of this contract and transfer funds from it.
     */
    function router() external view returns (address);

    /**
     * @dev Returns the current state hash of the program.
     */
    function stateHash() external view returns (bytes32);

    /**
     * @dev Returns the source for message ids unique generation.
     *      In-fact represents amount of messages received from Ethereum.
     *      Zeroed nonce is always represent init message.
     */
    function nonce() external view returns (uint256);

    /**
     * @dev Returns the bool flag indicates whether the program is exited.
     */
    function exited() external view returns (bool);

    /**
     * @dev Returns the address of the inheritor, which is set by the program on exit.
     *      Inheritor specifies the address to which all available program value should be transferred.
     */
    function inheritor() external view returns (address);

    /**
     * @dev Returns the address eligible to send first (init) message.
     */
    function initializer() external view returns (address);

    /* # Primary Gear logic (external calls) */

    /**
     * @dev Sends message to the program.
     *      As result of execution, the `MessageQueueingRequested` event will be emitted.
     * @param payload The payload of the message.
     * @param callReply Whether to set `call` flag in the reply message.
     * @return messageId Message ID of the sent message.
     */
    function sendMessage(bytes calldata payload, bool callReply) external payable returns (bytes32 messageId);

    /**
     * @dev Sends reply message to the program.
     *      Note that this function does not return `bytes32 messageId` of the sent message,
     *      if you want to calculate the `messageId` then use `gprimitives::MessageId::generate_reply(replied_to)`
     *      or use SDK in `ethexe/sdk/src/mirror.rs`.
     *      As result of execution, the `ReplyQueueingRequested` event will be emitted.
     * @param repliedTo Message ID to which the reply is sent.
     * @param payload The payload of the reply message.
     */
    function sendReply(bytes32 repliedTo, bytes calldata payload) external payable;

    /**
     * @dev Claim value from message in mailbox.
     *      As result of execution, the `ValueClaimingRequested` event will be emitted.
     * @param claimedId Message ID of the value to be claimed.
     */
    function claimValue(bytes32 claimedId) external;

    /**
     * @dev Tops up the executable balance of the program.
     *      As result of execution, the `ExecutableBalanceTopUpRequested` event will be emitted.
     * @param value The amount of WVARA to be transferred from user to `Router` as executable balance top up.
     */
    function executableBalanceTopUp(uint128 value) external;

    /**
     * @dev Transfers locked value to the inheritor.
     *      Note that this function can be called only after program exited.
     *      As result of execution, the `LockedValueTransferRequested` event will be emitted.
     */
    function transferLockedValueToInheritor() external;

    /* # Router-driven state and funds management */

    /**
     * @dev Initializes the contract with the given parameters.
     *      Note that ERC-1167 (Minimal Proxy Contract) does not support constructors by default,
     *      so we do the initialization separately after creating `Mirror` in this method.
     * @param initializer The address of the initializer. Only this address will be able to send the first (init) message.
     * @param abiInterface The address of the ABI interface. This address will be displayed as "proxy implementation"
     *        and is necessary to show the available methods of `Mirror` smart contract on Etherscan.
     *        In case it is a Sails framework smart contract, the user can set his own ABI.
     * @param isSmall The flag indicating if the program is small. See the description of `Mirror.isSmall` field for details.
     */
    function initialize(address initializer, address abiInterface, bool isSmall) external;

    /**
     * @dev Performs state transition for the `Mirror` contract.
     * @param transition The state transition data.
     * @return transitionHash The hash of the performed state transition.
     */
    function performStateTransition(Gear.StateTransition calldata transition)
        external
        payable
        returns (bytes32 transitionHash);
}
