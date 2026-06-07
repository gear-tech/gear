// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.35;

import {ERC1967Utils} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Utils.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";
import {Hashes} from "frost-secp256k1-evm/utils/cryptography/Hashes.sol";
import {ICallbacks} from "src/ICallbacks.sol";
import {IMirror} from "src/IMirror.sol";
import {IRouter} from "src/IRouter.sol";
import {IWrappedVara} from "src/IWrappedVara.sol";
import {BinaryMerkleTree} from "src/libraries/BinaryMerkleTree.sol";
import {Gear} from "src/libraries/Gear.sol";

/**
 * @dev Mirror smart contract is responsible for storing the minimal state of programs on our platform
 *      and transitioning from one state to another by calling `performStateTransition(...)`. It's built
 *      on actor-model architecture, and in Ethereum, we implement this through "request-response" model.
 *
 *      This means we have two types of events:
 *      - "Requested" events - when user calls one of the methods marked as "Primary Gear logic" we emit such an event,
 *        and all our nodes process it off-chain
 *      - "Responded" events - when we receive response from our nodes and transmit it back to Ethereum.
 *         All logic called within `performStateTransition(...)` and leading to methods marked as
 *         "Private calls related to performStateTransition" are such events.
 *
 *      It's important not to confuse these two, as this is how we implement the actor model in Ethereum.
 *
 *      Mirror economic model has two balances:
 *      - Owned balance in the native currency (ETH) and is represented as `u128`, since no amount of ETH can exceed `u128::MAX`.
 *        This balance type can be topped up via `fallback() external payable` and is also used throughout the protocol as `value`.
 *      - Executable balance in the ERC20 WVARA token is also represented as `u128`, since we also represent it as `u128` on our chain.
 *        It is used only in the `executableBalanceTopUp(...)` method to top up the executable balance of program on our platform.
 *        You must top up this balance type, since it allows the program to execute. Developers of WASM smart contracts on the
 *        Sails framework must develop revenue model for their dApp and top up the program's executable balance so that users
 *        can use it for free. This is called the "reverse-gas model". Developer can also require the presence of `value` in
 *        the owned balance when calling methods in a WASM smart contract to protect their program from spam.
 */
contract Mirror is IMirror {
    /**
     * @dev Special address to which Sails contract sends messages so that Mirror can decode events
     *      and re-remit then as Solidity events:
     *      - https://github.com/gear-tech/sails/blob/master/rs/src/solidity.rs
     */
    address internal constant ETH_EVENT_ADDR = 0xFFfFfFffFFfffFFfFFfFFFFFffFFFffffFfFFFfF;

    /**
     * @dev `uint8 discriminant` bit shift.
     */
    uint256 internal constant DISCRIMINANT_BIT_SHIFT = 248;
    /**
     * @dev `address destination` bit shift.
     */
    uint256 internal constant DESTINATION_BIT_SHIFT = 96;
    /**
     * @dev `uint128 value` bit shift.
     */
    uint256 internal constant VALUE_BIT_SHIFT = 128;
    /**
     * @dev `bool call` bit shift.
     */
    uint256 internal constant CALL_BIT_SHIFT = 120;
    /**
     * @dev `bytes4 replyCode` bit shift.
     */
    uint256 internal constant REPLY_CODE_BIT_SHIFT = 88;

    /**
     * @dev Mailboxed message discriminant.
     */
    uint256 internal constant MAILBOXED_MESSAGE = 0x00;
    /**
     * @dev Reply message discriminant.
     */
    uint256 internal constant REPLY_MESSAGE = 0x01;
    /**
     * @dev Value claim discriminant.
     */
    uint256 internal constant VALUE_CLAIM = 0x02;

    /**
     * @dev `uint8 discriminant` size.
     */
    uint256 internal constant DISCRIMINANT_SIZE = 1;
    /**
     * @dev `bytes32 messageId` size.
     */
    uint256 internal constant MESSAGE_ID_SIZE = 32;
    /**
     * @dev `address destination` size.
     */
    uint256 internal constant DESTINATION_SIZE = 20;
    /**
     * @dev `uint128 value` size.
     */
    uint256 internal constant VALUE_SIZE = 16;
    /**
     * @dev `bool call` size.
     */
    uint256 internal constant CALL_SIZE = 1;
    /**
     * @dev `bytes4 replyCode` size.
     */
    uint256 internal constant REPLY_CODE_SIZE = 4;
    /**
     * @dev `bytes32 replyTo` size.
     */
    uint256 internal constant REPLY_TO_SIZE = 32;

    /**
     * @dev `MESSAGE_ID_SIZE + DESTINATION_SIZE + VALUE_SIZE` common header size.
     */
    uint256 internal constant COMMON_HEADER_SIZE = 68;

    /**
     * @dev `DISCRIMINANT_SIZE` offset.
     */
    uint256 internal constant OFFSET1 = 1;
    /**
     * @dev `DISCRIMINANT_SIZE + MESSAGE_ID_SIZE` offset.
     */
    uint256 internal constant OFFSET2 = 33;
    /**
     * @dev `DISCRIMINANT_SIZE + MESSAGE_ID_SIZE + DESTINATION_SIZE` offset.
     */
    uint256 internal constant OFFSET3 = 53;
    /**
     * @dev `DISCRIMINANT_SIZE + MESSAGE_ID_SIZE + DESTINATION_SIZE + VALUE_SIZE + CALL_SIZE + REPLY_CODE_SIZE` offset.
     */
    uint256 internal constant OFFSET4 = 74;

    /**
     * @dev `DISCRIMINANT_SIZE + MESSAGE_ID_SIZE + DESTINATION_SIZE + VALUE_SIZE + CALL_SIZE` size.
     */
    uint256 internal constant MAILBOXED_MESSAGE_SIZE = 70;
    /**
     * @dev `DISCRIMINANT_SIZE + MESSAGE_ID_SIZE + DESTINATION_SIZE + VALUE_SIZE + CALL_SIZE + REPLY_CODE_SIZE + REPLY_TO_SIZE` size.
     */
    uint256 internal constant REPLY_MESSAGE_SIZE = 106;
    /**
     * @dev `DISCRIMINANT_SIZE + MESSAGE_ID_SIZE + DESTINATION_SIZE + VALUE_SIZE` size.
     */
    uint256 internal constant VALUE_CLAIM_SIZE = 69;

    /**
     * @dev Address of the `Router` contract, which is the sole authority
     *      to modify the state of this contract and transfer funds from it.
     * forge-lint: disable-next-item(screaming-snake-case-immutable)
     */
    address public immutable router;

    /**
     * @dev Program's current state hash.
     */
    bytes32 public stateHash;

    /**
     * @dev Source for message ids unique generation.
     *      In-fact represents amount of messages received from Ethereum.
     *      Zeroed nonce is always represent init message.
     */
    uint256 public nonce;

    /**
     * @dev The bool flag indicates whether the program is exited.
     */
    bool public exited;

    // TODO (breathx): consider proxying there.
    /**
     * @dev The address of the inheritor, which is set by the program on exit.
     *      Inheritor specifies the address to which all available program value should be transferred.
     */
    address public inheritor;

    /**
     * @dev The address eligible to send first (init) message.
     */
    address public initializer;

    /**
     * @dev Flag that indicates what type this `Mirror` smart contract is:
     *      - If `false`, it means that `Mirror` (clone) uses the `MirrorProxy` implementation
     *        (which is usually more expensive in terms of gas to create). This is generally the
     *        more popular way and is the one you will most likely use if you are writing programs using the Sails framework.
     *        This also means that all unknown selectors (calls) in `Mirror` will be processed in `Mirror.fallback()` and
     *        new message will be created for them implicitly via `_sendMessage(msg.data, callReply)`.
     *
     *        User writes WASM smart contract on Sails framework called "Сounter":
     *        - https://github.com/gear-foundation/vara-eth-demo/blob/master/app/src/lib.rs
     *
     *        User uploads WASM to Ethereum network via the call `IRouter(router).requestCodeValidation(bytes32 _codeId)`
     *        and waits for the code to be validated.
     *
     *        User also generates "Solidity ABI Interface" to allow incrementing counter or calling other methods within WASM smart contract.
     *        Next, we assume user uploads `CounterAbi` smart contract to Ethereum:
     *        ```solidity
     *        interface ICounter {
     *            function init(bool _callReply, uint32 counter) external returns (bytes32 messageId);
     *
     *            function counterAdd(bool _callReply, uint32 value) external returns (bytes32 messageId);
     *
     *            // ... other methods
     *        }
     *
     *        contract CounterAbi is ICounter {
     *            function init(bool _callReply, uint32 counter) external returns (bytes32 messageId) {}
     *
     *            function counterAdd(bool _callReply, uint32 value) external returns (bytes32 messageId) {}
     *        }
     *        ```
     *
     *        User calls `IRouter(router).createProgramWithAbiInterface(bytes32 _codeId, bytes32 _salt, address _overrideInitializer, address _abiInterface)`,
     *        where `_abiInterface = address(CounterAbi)`. See how `Mirror.initialize(...)` works; it will set `CounterAbi` as "proxy implementation",
     *        and Etherscan will think that `Mirror` has `CounterAbi` methods.
     *
     *        User can use any Ethereum-compatible client (Alloy, Viem, Ethers) to call method on the smart contract:
     *        `ICounter(mirror).counterAdd(bool _callReply=false, uint32 value=42)`, the client will automatically encode the call
     *        and send it, but in this case the `!isSmall` flag in `Mirror.fallback()` will be triggered, which will force `Mirror`
     *        to create new message and pass the Solidity call to the WASM smart contract on the Sails framework.
     *
     *        WASM smart contract will send reply and we will process it in `Mirror.performStateTransition(...)`.
     *      - If `true`, it means that `Mirror` (clone) uses the `MirrorProxySmall` implementation
     *        (which is usually less expensive in terms of gas to create). This case is suitable if the user develops
     *        WASM smart contracts using lower-level libraries like `gstd` / `gcore`. This also means that all unknown selectors
     *        (calls) in `Mirror` will NOT be processed in `Mirror.fallback()`.
     */
    bool isSmall;

    mapping(bytes32 stateHash => bytes32 merkleRoot) private _merkleRoots;
    mapping(bytes32 messageId => bool isProcessed) private _processedMessages;

    /**
     * @dev Minimal constructor that only sets the immutable `Router` address.
     * @param _router The address of the `Router` contract.
     */
    constructor(address _router) {
        router = _router;
    }

    /* # Modifiers */

    /**
     * @dev Functions marked with this modifier can only be called if the init message has been created before.
     */
    modifier onlyAfterInitMessage() {
        _onlyAfterInitMessage();
        _;
    }

    /**
     * @dev Internal function to check if the init message has been created before.
     */
    function _onlyAfterInitMessage() internal view {
        require(nonce > 0, InitMessageNotCreated());
    }

    /**
     * @dev Functions marked with this modifier can only be called if the init message has been created before or the caller is the initializer.
     */
    modifier onlyAfterInitMessageOrInitializer() {
        _onlyAfterInitMessageOrInitializer();
        _;
    }

    /**
     * @dev Internal function to check if the init message has been created before or the caller is the initializer.
     */
    function _onlyAfterInitMessageOrInitializer() internal view {
        require(nonce > 0 || msg.sender == initializer, InitMessageNotCreatedAndCallerNotInitializer());
    }

    /**
     * @dev Functions marked with this modifier can only be called if program is active.
     */
    modifier onlyIfActive() {
        _onlyIfActive();
        _;
    }

    /**
     * @dev Internal function to check if the program is active.
     */
    function _onlyIfActive() internal view {
        require(!exited, ProgramExited());
    }

    /**
     * @dev Functions marked with this modifier can only be called if program is exited.
     */
    modifier onlyIfExited() {
        _onlyIfExited();
        _;
    }

    /**
     * @dev Internal function to check if the program is exited.
     */
    function _onlyIfExited() internal view {
        require(exited, ProgramNotExited());
    }

    /**
     * @dev Functions marked with this modifier can only be called by the `Router`.
     */
    modifier onlyRouter() {
        _onlyRouter();
        _;
    }

    /**
     * @dev Internal function to check if the caller is the `Router`.
     */
    function _onlyRouter() internal view {
        require(msg.sender == router, CallerNotRouter());
    }

    /**
     * @dev Functions marked with this modifier can only be called when the `Router` is not paused.
     */
    modifier whenNotPaused() {
        _whenNotPaused();
        _;
    }

    /**
     * @dev Internal function to check if the `Router` is not paused.
     */
    function _whenNotPaused() internal view {
        require(!IRouter(router).paused(), EnforcedPause());
    }

    /**
     * @dev Non-zero Vara value must be transferred from source to `Router` in functions marked with this modifier.
     */
    modifier retrievingVara(uint128 value) {
        _retrievingVara(value);
        _;
    }

    /**
     * @dev Internal function to transfer non-zero Vara value from source to `Router`.
     */
    function _retrievingVara(uint128 value) internal {
        if (value != 0) {
            bool success = _wvara(router).transferFrom(msg.sender, router, value);
            require(success, WVaraTransferFailed());
        }
    }

    /**
     * @dev Non-zero Ether value must be transferred from source to `Router` in functions marked with this modifier.
     */
    function _retrievingEther(uint128 value) internal {
        if (value != 0) {
            (bool success,) = router.call{value: value}("");
            require(success, EtherTransferToRouterFailed());
        }
    }

    /* # View functions */

    /**
     * @dev Returns the outgoing actions merkle root for the specified state hash.
     *      Returns `bytes32(0)` if no merkle root was provided for the given state hash.
     * @param _stateHash Target state hash.
     * @return merkleRoot Outgoing actions merkle root for the specified state hash.
     */
    function getOutgoingActionsMerkleRoot(bytes32 _stateHash) external view returns (bytes32) {
        return _merkleRoots[_stateHash];
    }

    /**
     * @dev Checks if outgoing action was already processed.
     * @param _messageId Message ID to check.
     * @return isProcessed `true` if outgoing action was already processed, `false` otherwise.
     */
    function isOutgoingActionsProcessed(bytes32 _messageId) external view returns (bool) {
        return _processedMessages[_messageId];
    }

    /* # Primary Gear logic (external calls) */

    /**
     * @dev Sends message to the program.
     *      As result of execution, the `MessageQueueingRequested` event will be emitted.
     * @param _payload The payload of the message.
     * @param _callReply Whether to set `call` flag in the reply message.
     * @return messageId Message ID of the sent message.
     */
    function sendMessage(bytes calldata _payload, bool _callReply)
        external
        payable
        whenNotPaused
        returns (bytes32 messageId)
    {
        return _sendMessage(_payload, _callReply);
    }

    /**
     * @dev Sends reply message to the program.
     *      Note that this function does not return `bytes32 messageId` of the sent message,
     *      if you want to calculate the `messageId` then use `gprimitives::MessageId::generate_reply(replied_to)`
     *      or use SDK in `ethexe/sdk/src/mirror.rs`.
     *      As result of execution, the `ReplyQueueingRequested` event will be emitted.
     * @param _repliedTo Message ID to which the reply is sent.
     * @param _payload The payload of the reply message.
     */
    function sendReply(bytes32 _repliedTo, bytes calldata _payload)
        external
        payable
        whenNotPaused
        onlyIfActive
        onlyAfterInitMessage
    {
        uint128 _value = uint128(msg.value);

        _retrievingEther(_value);

        emit ReplyQueueingRequested(_repliedTo, msg.sender, _payload, _value);
    }

    // TODO (breathx): consider and support claimValue after exit.
    /**
     * @dev Claim value from message in mailbox.
     *      As result of execution, the `ValueClaimingRequested` event will be emitted.
     * @param _claimedId Message ID of the value to be claimed.
     */
    function claimValue(bytes32 _claimedId) external whenNotPaused onlyIfActive onlyAfterInitMessage {
        emit ValueClaimingRequested(_claimedId, msg.sender);
    }

    /**
     * @dev Tops up the executable balance of the program.
     *      As result of execution, the `ExecutableBalanceTopUpRequested` event will be emitted.
     * @param _value The amount of WVARA ERC20 token to be transferred from user to `Router` as executable balance top up.
     */
    function executableBalanceTopUp(uint128 _value) external whenNotPaused onlyIfActive retrievingVara(_value) {
        emit ExecutableBalanceTopUpRequested(_value);
    }

    /**
     * @dev Tops up the executable balance of the program.
     *      Unlike `Mirror.executableBalanceTopUp(...)`, this method allows to transfer WVARA ERC20 token from user to `Router`
     *      using permit signature, which can save one transaction for user.
     *      As result of execution, the `ExecutableBalanceTopUpRequested` event will be emitted.
     * @param _value The amount of WVARA ERC20 token to be transferred from user to `Router` as executable balance top up.
     * @param _deadline Deadline for the transaction to be executed.
     * @param _v ECDSA signature parameter.
     * @param _r ECDSA signature parameter.
     * @param _s ECDSA signature parameter.
     */
    function executableBalanceTopUpWithPermit(uint128 _value, uint256 _deadline, uint8 _v, bytes32 _r, bytes32 _s)
        external
        whenNotPaused
        onlyIfActive
    {
        try _wvara(router).permit(msg.sender, address(this), _value, _deadline, _v, _r, _s) {} catch {}
        _retrievingVara(_value);

        emit ExecutableBalanceTopUpRequested(_value);
    }

    /**
     * @dev Transfers locked value to the inheritor.
     *      Note that this function can be called only after program exited.
     *      As result of execution, the `LockedValueTransferRequested` event will be emitted.
     */
    function transferLockedValueToInheritor() external whenNotPaused {
        (, bool success) = _transferLockedValueToInheritor();
        require(success, TransferLockedValueToInheritorExternalFailed());
    }

    /* # Primary Gear logic (external calls, pull-based methods) */

    /**
     * @dev Processes outgoing action.
     * @param _stateHash The state hash for which to process outgoing action.
     * @param _totalLeaves The total number of leaves in the merkle tree.
     * @param _leafIndex The index of the leaf for which to process outgoing action.
     * @param _payload The payload for the outgoing action.
     * @param _proof The merkle proof for the claim.
     */
    function processOutgoingAction(
        bytes32 _stateHash,
        uint256 _totalLeaves,
        uint256 _leafIndex,
        bytes calldata _payload,
        bytes32[] calldata _proof
    ) external {
        require(
            _tryParseAndProcessOutgoingAction(_stateHash, _totalLeaves, _leafIndex, _payload, _proof),
            OutgoingActionInvalidPayload()
        );
    }

    /* # Router-driven state and funds management */

    /**
     * @dev Initializes the contract with the given parameters.
     *      Note that ERC-1167 (Minimal Proxy Contract) does not support constructors by default,
     *      so we do the initialization separately after creating `Mirror` in this method.
     * @param _initializer The address of the initializer. Only this address will be able to send the first (init) message.
     * @param _abiInterface The address of the ABI interface. This address will be displayed as "proxy implementation"
     *        and is necessary to show the available methods of `Mirror` smart contract on Etherscan.
     *        In case it is a Sails framework smart contract, the user can set his own ABI.
     * @param _isSmall The flag indicating if the program is small. See the description of `Mirror.isSmall` field for details.
     * @param _initialExecutableBalance The initial executable balance to be transferred to the program.
     */
    function initialize(address _initializer, address _abiInterface, bool _isSmall, uint128 _initialExecutableBalance)
        external
        onlyRouter
    {
        require(initializer == address(0), InitializerAlreadySet());

        require(!isSmall, IsSmallAlreadySet());

        StorageSlot.AddressSlot storage implementationSlot =
            StorageSlot.getAddressSlot(ERC1967Utils.IMPLEMENTATION_SLOT);

        require(implementationSlot.value == address(0), AbiInterfaceAlreadySet());

        initializer = _initializer;
        isSmall = _isSmall;
        implementationSlot.value = _abiInterface;

        if (_initialExecutableBalance != 0) {
            emit ExecutableBalanceTopUpRequested(_initialExecutableBalance);
        }
    }

    /**
     * @dev Performs state transition for the `Mirror` contract.
     * @param _transition The state transition data.
     * @return transitionHash The hash of the performed state transition.
     */
    function performStateTransition(Gear.StateTransition calldata _transition)
        external
        payable
        onlyRouter
        returns (bytes32 transitionHash)
    {
        /**
         * @dev Verify that the transition belongs to this contract.
         */
        require(_transition.actorId == address(this), InvalidActorId());

        /**
         * @dev Transfer value to router if valueToReceive is non-zero and has negative sign.
         */
        if (_transition.valueToReceiveNegativeSign) {
            _retrievingEther(_transition.valueToReceive);
        }

        /**
         * @dev Send all outgoing messages.
         */
        bytes32 messagesHashesHash = _sendMessages(_transition.messages);

        /**
         * @dev Sets merkle root of outgoing actions for the new state hash.
         */
        _updateOutgoingActionsMerkleRoot(_transition.newStateHash, _transition.merkleRoot);

        /**
         * @dev Set inheritor if exited.
         */
        if (_transition.exited) {
            _setInheritor(_transition.inheritor);
        } else {
            require(_transition.inheritor == address(0), InheritorMustBeZero());
        }

        /**
         * @dev Update the state hash if changed.
         */
        if (stateHash != _transition.newStateHash) {
            _updateStateHash(_transition.newStateHash);
        }

        /**
         * @dev Return hash of performed state transition.
         */
        return Gear.stateTransitionHash(
            _transition.actorId,
            _transition.newStateHash,
            _transition.exited,
            _transition.inheritor,
            _transition.valueToReceive,
            _transition.valueToReceiveNegativeSign,
            _transition.merkleRoot,
            messagesHashesHash
        );
    }

    /* # Private calls, related to primary Gear logic */

    // TODO: add documentation for this function
    function _tryParseAndProcessOutgoingAction(
        bytes32 _stateHash,
        uint256 _totalLeaves,
        uint256 _leafIndex,
        bytes calldata _payload,
        bytes32[] calldata _proof
    ) private returns (bool) {
        if (!(_payload.length > 0)) {
            return false;
        }

        uint256 discriminant;
        assembly ("memory-safe") {
            // `DISCRIMINANT_BIT_SHIFT` right bit shift is required to remove extra bits since `calldataload` returns `uint256`
            discriminant := shr(DISCRIMINANT_BIT_SHIFT, calldataload(_payload.offset))
        }

        // TODO: support more discriminants when implementing mailboxed and reply messages
        /*if (!(discriminant >= MAILBOXED_MESSAGE && discriminant <= VALUE_CLAIM)) {
            return false;
        }*/
        if (!(discriminant == VALUE_CLAIM)) {
            return false;
        }

        if (!(_payload.length > COMMON_HEADER_SIZE)) {
            return false;
        }

        // we use offset `OFFSET1 = DISCRIMINANT_SIZE` to skip `uint8 discriminant`
        bytes32 messageId;
        assembly ("memory-safe") {
            messageId := calldataload(add(_payload.offset, OFFSET1))
        }

        require(!_processedMessages[messageId], OutgoingActionAlreadyProcessed(messageId));

        // we use offset `OFFSET2 = DISCRIMINANT_SIZE + MESSAGE_ID_SIZE` to skip `uint8 discriminant` and `bytes32 messageId`
        address destination;
        assembly ("memory-safe") {
            // `DESTINATION_BIT_SHIFT` right bit shift is required to remove extra bits since `calldataload` returns `uint256`
            destination := shr(DESTINATION_BIT_SHIFT, calldataload(add(_payload.offset, OFFSET2)))
        }

        // we use offset `OFFSET3 = DISCRIMINANT_SIZE + MESSAGE_ID_SIZE + DESTINATION_SIZE` to skip `uint8 discriminant`, `bytes32 messageId`,
        // `address destination`
        uint256 word;
        assembly ("memory-safe") {
            word := calldataload(add(_payload.offset, OFFSET3))
        }

        // casting to 'uint128' is safe because value is represented as `uint128` on our chain
        // forge-lint: disable-next-line(unsafe-typecast)
        uint128 value = uint128(word >> VALUE_BIT_SHIFT);

        if (discriminant == VALUE_CLAIM) {
            if (!(_payload.length == VALUE_CLAIM_SIZE)) {
                return false;
            }
        }

        // casting to 'uint8' is safe because `bool call` is represented as `uint8`
        // forge-lint: disable-next-line(unsafe-typecast)
        bool call = uint8(word >> CALL_BIT_SHIFT) != 0;
        // casting to 'uint32' is safe because `bytes4 replyCode` is represented as `uint32`
        // forge-lint: disable-next-line(unsafe-typecast)
        bytes4 replyCode = bytes4(uint32(word >> REPLY_CODE_BIT_SHIFT));

        bytes calldata payload = _payload[:0]; // empty payload by default
        if (discriminant == MAILBOXED_MESSAGE) {
            if (!(_payload.length >= MAILBOXED_MESSAGE_SIZE)) {
                return false;
            }
            payload = _payload[MAILBOXED_MESSAGE_SIZE:];
        } else if (discriminant == REPLY_MESSAGE) {
            if (!(_payload.length >= REPLY_MESSAGE_SIZE)) {
                return false;
            }
            payload = _payload[REPLY_MESSAGE_SIZE:];
        }

        // we use offset `OFFSET4 = DISCRIMINANT_SIZE + MESSAGE_ID_SIZE + DESTINATION_SIZE + VALUE_SIZE + CALL_SIZE + REPLY_CODE_SIZE` to skip `uint8 discriminant`, `bytes32 messageId`,
        // `address destination`, `uint128 value`, `bool call`, `bytes4 replyCode`
        bytes32 replyTo;
        assembly ("memory-safe") {
            replyTo := calldataload(add(_payload.offset, OFFSET4))
        }

        bytes32 merkleRoot = _merkleRoots[_stateHash];
        require(merkleRoot != bytes32(0), OutgoingActionMerkleRootNotFound(_stateHash));

        bytes32 outgoingActionHash;

        if (discriminant == MAILBOXED_MESSAGE) {
            // TODO: implement hash for mailboxed message
        } else if (discriminant == REPLY_MESSAGE) {
            // TODO: implement hash for reply message
        } else if (discriminant == VALUE_CLAIM) {
            outgoingActionHash = Gear.valueClaimHash(messageId, destination, value);
        }

        require(
            BinaryMerkleTree.verifyProofCalldata(merkleRoot, _proof, _totalLeaves, _leafIndex, outgoingActionHash),
            OutgoingActionInvalidMerkleProof()
        );

        _processedMessages[messageId] = true;

        if (discriminant == MAILBOXED_MESSAGE) {
            // TODO: implement mailboxed message
        } else if (discriminant == REPLY_MESSAGE) {
            // TODO: implement reply message
        } else if (discriminant == VALUE_CLAIM) {
            // TODO: remove gas limit 5_000 after full migration to merkle roots
            //       currently it's ok bcz we don't use claims as smart-contracts
            bool success = _transferEther(destination, value);
            require(success, ValueClaimFailed(messageId, value));

            emit ValueClaimed(messageId, value);
        }

        return true;
    }

    /**
     * @dev Internal implementation of `sendMessage` function.
     *      This function is used to send message to the program and emit `MessageQueueingRequested` event.
     * @param _payload The payload of the message.
     * @param _callReply Whether to set `call` flag in the reply message.
     * @return messageId Message ID of the sent message.
     */
    function _sendMessage(bytes calldata _payload, bool _callReply)
        private
        onlyIfActive
        onlyAfterInitMessageOrInitializer
        returns (bytes32 messageId)
    {
        uint128 _value = uint128(msg.value);

        _retrievingEther(_value);

        uint256 _nonce = nonce;
        /**
         * @dev Generate unique message ID by formula:
         *      - `keccak256(abi.encodePacked(address(this), nonce++))`
         */
        bytes32 id;
        assembly ("memory-safe") {
            mstore(0x00, shl(96, address()))
            mstore(0x14, _nonce)
            id := keccak256(0x00, 0x34)
        }
        nonce++;

        emit MessageQueueingRequested(id, msg.sender, _payload, _value, _callReply);

        return id;
    }

    /**
     * @dev Internal implementation of `transferLockedValueToInheritor` function.
     *      Note that this function can be called only after program exited.
     * @return valueTransferred The amount of WVARA transferred.
     * @return transferSuccess The flag indicating if the transfer was successful.
     */
    function _transferLockedValueToInheritor()
        private
        onlyIfExited
        returns (uint128 valueTransferred, bool transferSuccess)
    {
        uint256 balance = address(this).balance;
        // casting to 'uint128' is safe because ETH supply is less than `type(uint128).max`
        // forge-lint: disable-next-line(unsafe-typecast)
        uint128 balance128 = uint128(balance);
        return (balance128, _transferEther(inheritor, balance128));
    }

    /* # Private calls, related to performStateTransition */

    // TODO (breathx): consider when to emit event: on success in decoder, on failure etc.
    // TODO (breathx): make decoder gas configurable.
    // TODO (breathx): handle if goes to mailbox or not.
    /**
     * @dev Internal implementation of `_sendMessages` function.
     *      It sends all outgoing messages from the `Mirror` contract and emits appropriate events.
     * @param _messages The array of messages to be sent.
     * @return messagesHash The hash of the sent messages.
     */
    function _sendMessages(Gear.Message[] calldata _messages) private returns (bytes32) {
        uint256 messagesLen = _messages.length;
        uint256 messagesHashesSize = messagesLen * 32;
        uint256 messagesHashesMemPtr = Memory.allocate(messagesHashesSize);
        uint256 offset = 0;

        for (uint256 i = 0; i < messagesLen; i++) {
            Gear.Message calldata message = _messages[i];

            /**
             * @dev Generate hash for the message.
             */
            bytes32 messageHash = Gear.messageHash(message);
            /**
             * @dev Store the message hash in memory at messagesHashes[offset : offset+32].
             */
            Memory.writeWordAsBytes32(messagesHashesMemPtr, offset, messageHash);
            unchecked {
                offset += 32;
            }

            /**
             * @dev Send the message based on its type (`Gear.Message` or `Gear.Reply`).
             */
            if (message.replyDetails.to == 0) {
                _sendMailboxedMessage(message);
            } else {
                _sendReplyMessage(message);
            }
        }

        return Hashes.efficientKeccak256AsBytes32(messagesHashesMemPtr, 0, messagesHashesSize);
    }

    /**
     * @dev Internal function to send message that goes to mailbox.
     *      Value never sent since goes to mailbox.
     *      Emits `Message` event if it is not event from Sails framework.
     *      If `_message.call = true`, then call will be made to `_message.destination`
     *      with _message.payload and gas limit of 500_000 to prevent DoS attacks.
     *      If call fails, then `MessageCallFailed` event will be emitted.
     * @param _message The message to be sent.
     */
    function _sendMailboxedMessage(Gear.Message calldata _message) private {
        /**
         * @dev First, we'll try to parse event from the Sails framework
         *      and then emit it on behalf of the `Mirror` smart contract.
         */
        if (!_tryParseAndEmitSailsEvent(_message)) {
            // TODO #5243: We currently support this on the `Mirror` smart contract,
            // but we don't support it on the Rust client and we don't have corresponding syscalls for it.
            // This is unreachable code branch currently.
            if (_message.call) {
                (bool success,) = _message.destination.call{gas: 500_000}(_message.payload);

                if (!success) {
                    /**
                     * @dev In case of failed call, we emit appropriate event to inform external users.
                     */
                    emit MessageCallFailed(_message.id, _message.destination, _message.value);
                    return;
                }
            }

            emit Message(_message.id, _message.destination, _message.payload, _message.value);
        }
    }

    /**
     * @dev Tries to parse an event from the Sails framework and emit it in Solidity notation.
     *
     *      User writes WASM smart contract on Sails framework called "Counter":
     *      - https://github.com/gear-foundation/vara-eth-demo/blob/master/app/src/lib.rs
     *
     *      Example of defining Solidity events in WASM contract based on Sails framework:
     *      ```rust
     *      #[event]
     *      #[derive(Clone, Debug, PartialEq, Encode, TypeInfo)]
     *      #[codec(crate = scale_codec)]
     *      #[scale_info(crate = scale_info)]
     *      pub enum CounterEvents {
     *          Added {
     *              #[indexed]
     *              source: ActorId,
     *              value: u32,
     *          },
     *      }
     *      ```
     *
     *      User also generates "Solidity ABI interface" that allows services like Etherscan to decode events from `Mirror`
     *      (since we use the ABI interface as "proxy implementation"):
     *      ```solidity
     *      interface ICounter {
     *          event Added(address indexed source, uint32 value);
     *
     *          // ... other events
     *      }
     *      ```
     *
     *      Now let's imagine that the user wants to calculate something in WASM contract and send it to Ethereum as event,
     *      which will then be emitted by `Mirror` smart contract as showed on services like Etherscan:
     *      ```rust
     *      #[service(events = CounterEvents)]
     *      impl CounterService<'_> {
     *          #[export]
     *          pub fn add(&mut self, value: u32) -> u32 {
     *              let mut data_mut = self.data.borrow_mut();
     *              data_mut.counter = data_mut.counter.checked_add(value).expect("failed to add");
     *              let source = Syscall::message_source();
     *              self.emit_eth_event(CounterEvents::Added { source, value })
     *                  .expect("failed to emit eth event");
     *              data_mut.counter
     *          }
     *      }
     *      ```
     *
     *      All the `emit_eth_event` method in the Sails framework does is call the syscall
     *      `gcore::msg::send(destination=ETH_EVENT_ADDR, payload, value=0)`, where `payload`
     *      is encoded in Solidity notation as described below.
     *
     *      Format in which the Sails framework sends events:
     *      - `uint8 topicsLength` (can be `1`, `2`, `3`, `4`).
     *         specifies which opcode (`log1`, `log2`, `log3`, `log4`) should be called.
     *      - `bytes32 topic1` (required)
     *         should never match our event selectors!
     *      - `bytes32 topic2` (optional)
     *      - `bytes32 topic3` (optional)
     *      - `bytes32 topic4` (optional)
     *      - `bytes payload` (optional)
     *         contains encoded data of event in form of `abi.encode(...)`.
     * @param _message The message to be parsed and emitted as Solidity event.
     * @return isSailsEvent `true` in case of success and `false` in case of error (no matching event found).
     */
    function _tryParseAndEmitSailsEvent(Gear.Message calldata _message) private returns (bool isSailsEvent) {
        bytes calldata payload = _message.payload;

        if (!(_message.destination == ETH_EVENT_ADDR && _message.value == 0 && payload.length > 0)) {
            return false;
        }

        uint256 topicsLength;
        assembly ("memory-safe") {
            /**
             * @dev `248` right bit shift is required to remove extra bits since `calldataload` returns `uint256`
             */
            topicsLength := shr(248, calldataload(payload.offset))
        }

        if (!(topicsLength >= 1 && topicsLength <= 4)) {
            return false;
        }

        uint256 topicsLengthInBytes;
        unchecked {
            topicsLengthInBytes = 1 + topicsLength * 32;
        }

        if (!(payload.length >= topicsLengthInBytes)) {
            return false;
        }

        /**
         * @dev We use offset 1 to skip `uint8 topicsLength`
         */
        bytes32 topic1;
        assembly ("memory-safe") {
            topic1 := calldataload(add(payload.offset, 1))
        }

        /**
         * @dev SECURITY:
         *      Very important check because custom events can match our hashes!
         *      If we miss even 1 event that is emitted by Mirror, user will be able to fake protocol logic!
         *
         *      Command to re-generate selectors check:
         *      ```bash
         *      grep -Po "    event\s+\K[^(]+" ethexe/contracts/src/IMirror.sol | xargs -I{} echo "            topic1 != {}.selector &&" | sed '$ s/ &&$//'
         *      ```
         */
        // forgefmt: disable-start
        if (!(
            topic1 != StateChanged.selector &&
            topic1 != MessageQueueingRequested.selector &&
            topic1 != ReplyQueueingRequested.selector &&
            topic1 != ValueClaimingRequested.selector &&
            topic1 != OwnedBalanceTopUpRequested.selector &&
            topic1 != ExecutableBalanceTopUpRequested.selector &&
            topic1 != Message.selector &&
            topic1 != MessageCallFailed.selector &&
            topic1 != Reply.selector &&
            topic1 != ReplyCallFailed.selector &&
            topic1 != ValueClaimed.selector &&
            topic1 != TransferLockedValueToInheritorFailed.selector &&
            topic1 != ReplyTransferFailed.selector
        )) {
            return false;
        }
        // forgefmt: disable-end

        uint256 size;
        unchecked {
            size = payload.length - topicsLengthInBytes;
        }

        uint256 memPtr = Memory.allocate(size);
        assembly ("memory-safe") {
            calldatacopy(memPtr, add(payload.offset, topicsLengthInBytes), size)
        }

        /**
         * @dev We use offset 1 to skip `uint8 topicsLength`.
         *      Regular offsets: `32`, `64`, `96`.
         */
        bytes32 topic2;
        bytes32 topic3;
        bytes32 topic4;
        assembly ("memory-safe") {
            topic2 := calldataload(add(payload.offset, 33))
            topic3 := calldataload(add(payload.offset, 65))
            topic4 := calldataload(add(payload.offset, 97))
        }

        if (topicsLength == 1) {
            assembly ("memory-safe") {
                log1(memPtr, size, topic1)
            }
        } else if (topicsLength == 2) {
            assembly ("memory-safe") {
                log2(memPtr, size, topic1, topic2)
            }
        } else if (topicsLength == 3) {
            assembly ("memory-safe") {
                log3(memPtr, size, topic1, topic2, topic3)
            }
        } else if (topicsLength == 4) {
            assembly ("memory-safe") {
                log4(memPtr, size, topic1, topic2, topic3, topic4)
            }
        }

        return true;
    }

    /**
     * @dev Internal function to send reply message.
     *      Non-zero value always sent since never goes to mailbox.
     *      Emits `Reply` event if `_message.call = false`.
     *      If `_message.call = true`, the call will be made to `_message.destination` with
     *      gas limit of 500_000 to prevent DoS attacks and with `_message.value`.
     *      The `_message.replyDetails` will also be evaluated to determine the reply's success.
     *      If `gear_core::message::ReplyCode` is successful, `_message.payload` will be used.
     *      If unsuccessful, `payload = ICallbacks.onErrorReply(_message.id, _message.payload, _message.replyDetails.code)`
     *      will be used and the appropriate method on `_message.destination` will be called.
     *      Function will also always attempt to send `_message.value`. If this fails for some reason,
     *      the `ReplyTransferFailed` event will be emitted.
     *      If call fails, then `ReplyCallFailed` event will be emitted.
     *
     *      User writes WASM smart contract on Sails framework called "Counter":
     *      - https://github.com/gear-foundation/vara-eth-demo/blob/master/app/src/lib.rs
     *
     *      All the contract method does is return `u32` as result (reply):
     *      ```rust
     *      #[service(events = CounterEvents)]
     *      impl CounterService<'_> {
     *          #[export]
     *          pub fn add(&mut self, value: u32) -> u32 {
     *              let mut data_mut = self.data.borrow_mut();
     *              data_mut.counter = data_mut.counter.checked_add(value).expect("failed to add");
     *              let source = Syscall::message_source();
     *              self.emit_eth_event(CounterEvents::Added { source, value })
     *                  .expect("failed to emit eth event");
     *              data_mut.counter
     *          }
     *      }
     *
     *      User also generates "Solidity ABI Interface" to allow incrementing counter or calling other methods within WASM smart contract.
     *      Next, we assume user uploads `CounterAbi` smart contract to Ethereum:
     *      ```solidity
     *      interface ICounter {
     *          function init(bool _callReply, uint32 counter) external returns (bytes32 messageId);
     *
     *          function counterAdd(bool _callReply, uint32 value) external returns (bytes32 messageId);
     *
     *          // ... other methods
     *      }
     *
     *      contract CounterAbi is ICounter {
     *          function init(bool _callReply, uint32 counter) external returns (bytes32 messageId) {}
     *
     *          function counterAdd(bool _callReply, uint32 value) external returns (bytes32 messageId) {}
     *      }
     *      ```
     *
     *      User also generates "Solidity Callback Interface" and implements own `CounterCaller` smart contract,
     *      which will handle reply hooks in methods starting with `replyOn_`:
     *      ```solidity
     *      interface ICounterCallbacks {
     *          function replyOn_init(bytes32 messageId) external;
     *
     *          function replyOn_counterAdd(bytes32 messageId, uint32 reply) external;
     *
     *          // ... other methods
     *
     *          function onErrorReply(bytes32 messageId, bytes calldata payload, bytes4 replyCode) external payable;
     *      }
     *
     *      contract CounterCaller is ICounterCallbacks {
     *          ICounter public immutable MIRROR;
     *
     *          constructor(ICounter _mirror) {
     *              MIRROR = _mirror;
     *          }
     *
     *          modifier onlyMirror() {
     *              _onlyMirror();
     *              _;
     *          }
     *
     *          function _onlyMirror() internal view {
     *              require(msg.sender == address(MIRROR));
     *          }
     *
     *          // Call `Counter` constructor on our platform
     *
     *          function init(uint32 counter) external {
     *              // `bool _callReply = true`
     *              bytes32 _messageId = MIRROR.init(true, counter);
     *          }
     *
     *          function replyOn_init(bytes32 messageId) external onlyMirror {
     *              // ...
     *          }
     *
     *          // Compute `Counter.add(uint32 value) -> uint32 reply` on our platform
     *
     *          mapping(bytes32 messageId => bool knownMessage) public counterAddInputs;
     *          mapping(bytes32 messageId => uint32 output) public counterAddResults;
     *
     *          function counterAdd(uint32 value) external returns (bytes32 messageId) {
     *              // `bool _callReply = true`
     *              bytes32 _messageId = MIRROR.counterAdd(true, value);
     *              counterAddInputs[_messageId] = true;
     *              messageId = _messageId;
     *          }
     *
     *          function replyOn_counterAdd(bytes32 messageId, uint32 reply) external onlyMirror {
     *              counterAddResults[messageId] = reply;
     *          }
     *
     *          // Handle `Counter` errors on our platform
     *
     *          event ErrorReply(bytes32 messageId, bytes payload, bytes4 replyCode);
     *
     *          function onErrorReply(bytes32 messageId, bytes calldata payload, bytes4 replyCode)
     *              external
     *              payable
     *              onlyMirror
     *          {
     *              emit ErrorReply(messageId, payload, replyCode);
     *          }
     *      }
     *      ```
     *
     *      User calls `CounterCaller.counterAdd(uint32 value)`, and the smart contract calls `ICounter.counterAdd(bool _callReply=true, uint32 value)`.
     *      Result calculated in WASM smart contract on Sails framework in `Counter.add(uint32 value) -> uint32 reply` method will be passed to
     *      `replyOn_counterAdd(bytes32 messageId, uint32 reply)`.
     * @param _message The reply message to be sent.
     */
    function _sendReplyMessage(Gear.Message calldata _message) private {
        if (_message.call) {
            bool isSuccessReply = _message.replyDetails.code[0] == 0;

            bytes memory payload;

            if (isSuccessReply) {
                payload = _message.payload;
            } else {
                // TODO (breathx): this should be removed in favor of future sails impl.
                // TODO (breathx): consider support value arg.
                payload = abi.encodeWithSelector(
                    ICallbacks.onErrorReply.selector, _message.id, _message.payload, _message.replyDetails.code
                );
            }

            (bool success,) = _message.destination.call{gas: 500_000, value: _message.value}(payload);

            if (!success) {
                bool transferSuccess = _transferEther(_message.destination, _message.value);
                if (!transferSuccess) {
                    emit ReplyTransferFailed(_message.destination, _message.value);
                }

                /**
                 * @dev In case of failed call, we emit appropriate event to inform external users.
                 */
                emit ReplyCallFailed(_message.value, _message.replyDetails.to, _message.replyDetails.code);
            }
        } else {
            bool transferSuccess = _transferEther(_message.destination, _message.value);
            if (!transferSuccess) {
                emit ReplyTransferFailed(_message.destination, _message.value);
            }

            emit Reply(_message.payload, _message.value, _message.replyDetails.to, _message.replyDetails.code);
        }
    }

    // TODO (breathx): claimValues will fail if the program is exited: keep the funds on `Router`.
    /**
     * @dev Internal function to pass outgoing actions as merkle root.
     * @param _stateHash The state hash for which the merkle root of outgoing actions is set.
     * @param _merkleRoot The merkle root of outgoing actions for the state hash.
     */
    function _updateOutgoingActionsMerkleRoot(bytes32 _stateHash, bytes32 _merkleRoot) private {
        _merkleRoots[_stateHash] = _merkleRoot;
    }

    // TODO (breathx): allow zero inheritor in `Router`.
    /**
     * @dev Sets the inheritor address, sets exited flag to `true` and
     *      transfer all available balance to the inheritor.
     * @param _inheritor The address of the inheritor.
     */
    function _setInheritor(address _inheritor) private onlyIfActive {
        /**
         * @dev Set inheritor.
         */
        exited = true;
        inheritor = _inheritor;

        /**
         * @dev Transfer all available balance to the inheritor.
         */
        (uint128 value, bool success) = _transferLockedValueToInheritor();
        if (!success) {
            /**
             * @dev In case of failed transfer, we emit appropriate event to inform external users.
             */
            emit TransferLockedValueToInheritorFailed(_inheritor, value);
        }
    }

    /**
     * @dev Updates the state hash.
     * @param _stateHash The new state hash.
     */
    function _updateStateHash(bytes32 _stateHash) private {
        /**
         * @dev Set state hash.
         */
        stateHash = _stateHash;

        /**
         * @dev Emits an event signaling that the state has changed.
         */
        emit StateChanged(stateHash);
    }

    /* # Local helper functions */

    /**
     * @dev Get the `WrappedVara` contract instance.
     * @param routerAddr The address of the `Router` contract.
     */
    function _wvara(address routerAddr) private view returns (IWrappedVara) {
        address wvaraAddr = IRouter(routerAddr).wrappedVara();
        return IWrappedVara(wvaraAddr);
    }

    /**
     * @dev Transfer ETH to destination address.
     *      It has gas limit of 5_000 to prevent DoS attacks.
     * @param destination The address to transfer ETH to.
     * @param value The amount of ETH to transfer.
     */
    function _transferEther(address destination, uint128 value) private returns (bool) {
        if (value != 0) {
            (bool success,) = destination.call{gas: 5_000, value: value}("");
            return success;
        }
        return true;
    }

    /**
     * @dev Fallback function for top-up owned balance in native currency (ETH)
     *      and for sending arbitrary calls to `!isSmall` `Mirror` contracts
     *      as messages to Sails framework.
     *
     *      See the description of `Mirror.isSmall` field for details.
     */
    fallback() external payable whenNotPaused {
        if (msg.value > 0 && msg.data.length == 0) {
            uint128 value = uint128(msg.value);

            emit OwnedBalanceTopUpRequested(value);
        } else if (!isSmall && msg.data.length >= 0x24) {
            /**
             * @dev We only allow arbitrary calls to `!isSmall` `Mirror` contracts,
             *      which are more likely to come from their ABI interfaces.
             *
             *      The minimum call data length is 0x24 (36 bytes) because:
             *      - 0x04 (4 bytes) for the function selector   [0x00..0x04)
             *      - 0x20 (32 bytes) for the bool `callReply`   [0x04..0x24)
             */
            uint256 callReply;

            assembly ("memory-safe") {
                callReply := calldataload(0x04)
            }

            bytes32 messageId = _sendMessage(msg.data, callReply != 0);

            assembly ("memory-safe") {
                mstore(0x00, messageId)
                return(0x00, 0x20)
            }
        } else {
            revert InvalidFallbackCall();
        }
    }
}
