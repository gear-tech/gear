// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {ICallbacks} from "./ICallbacks.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {Gear} from "./libraries/Gear.sol";
import {ERC1967Utils} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Utils.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";
import {Hashes} from "frost-secp256k1-evm/utils/cryptography/Hashes.sol";

contract Mirror is IMirror {
    /// @dev Special address to which Sails contract sends messages so that Mirror can decode events:
    ///      https://github.com/gear-tech/sails/blob/master/rs/src/solidity.rs
    address internal constant ETH_EVENT_ADDR = 0xFFfFfFffFFfffFFfFFfFFFFFffFFFffffFfFFFfF;

    /// forge-lint: disable-next-item(screaming-snake-case-immutable)
    /// @dev Address of the router contract, which is the sole authority.
    address public immutable router;

    /// @dev Program's current state hash.
    bytes32 public stateHash;

    /// @dev Source for message ids unique generation.
    ///      In-fact represents amount of messages received from Ethereum.
    ///      Zeroed nonce is always represent init message.
    uint256 public nonce;

    /// @dev The bool flag indicates whether the program is exited.
    bool public exited;

    // TODO (breathx): consider proxying there.
    /// @dev The address of the inheritor, which is set by the program on exit.
    address public inheritor;

    /// @dev The address eligible to send first (init) message.
    address public initializer;

    /// @dev The bool flag indicates whether to process arbitrary calls as `sendMessage` payload.
    bool isSmall;

    /// @dev Minimal constructor that only sets the immutable router address.
    constructor(address _router) {
        router = _router;
    }

    /// @dev Functions marked with this modifier can only be called if the init message has been created before.
    modifier onlyAfterInitMessage() {
        _onlyAfterInitMessage();
        _;
    }

    function _onlyAfterInitMessage() internal view {
        require(nonce > 0, InitMessageNotCreated());
    }

    /// @dev Functions marked with this modifier can only be called if the init message has been created before or the caller is the initializer.
    modifier onlyAfterInitMessageOrInitializer() {
        _onlyAfterInitMessageOrInitializer();
        _;
    }

    function _onlyAfterInitMessageOrInitializer() internal view {
        require(nonce > 0 || msg.sender == initializer, InitMessageNotCreatedAndCallerNotInitializer());
    }

    /// @dev Functions marked with this modifier can only be called if program is active.
    modifier onlyIfActive() {
        _onlyIfActive();
        _;
    }

    function _onlyIfActive() internal view {
        require(!exited, ProgramExited());
    }

    /// @dev Functions marked with this modifier can only be called if program is exited.
    modifier onlyIfExited() {
        _onlyIfExited();
        _;
    }

    function _onlyIfExited() internal view {
        require(exited, ProgramNotExited());
    }

    /// @dev Functions marked with this modifier can only be called by the router.
    modifier onlyRouter() {
        _onlyRouter();
        _;
    }

    function _onlyRouter() internal view {
        require(msg.sender == router, CallerNotRouter());
    }

    /// @dev Non-zero Vara value must be transferred from source to router in functions marked with this modifier.
    modifier retrievingVara(uint128 value) {
        _retrievingVara(value);
        _;
    }

    function _retrievingVara(uint128 value) internal {
        if (value != 0) {
            bool success = _wvara(router).transferFrom(msg.sender, router, value);
            require(success, WVaraTransferFailed());
        }
    }

    /// @dev Non-zero Ether value must be transferred from source to router in functions marked with this modifier.
    function _retrievingEther(uint128 value) internal {
        if (value != 0) {
            (bool success,) = router.call{value: value}("");
            require(success, EtherTransferToRouterFailed());
        }
    }

    /* Primary Gear logic */

    function sendMessage(bytes calldata _payload, bool _callReply) external payable returns (bytes32) {
        return _sendMessage(_payload, _callReply);
    }

    function sendReply(bytes32 _repliedTo, bytes calldata _payload) external payable onlyIfActive onlyAfterInitMessage {
        uint128 _value = uint128(msg.value);

        _retrievingEther(_value);

        emit ReplyQueueingRequested(_repliedTo, msg.sender, _payload, _value);
    }

    // TODO (breathx): consider and support claimValue after exit.
    function claimValue(bytes32 _claimedId) external onlyIfActive onlyAfterInitMessage {
        emit ValueClaimingRequested(_claimedId, msg.sender);
    }

    function executableBalanceTopUp(uint128 _value) external onlyIfActive retrievingVara(_value) {
        emit ExecutableBalanceTopUpRequested(_value);
    }

    function transferLockedValueToInheritor() public onlyIfExited {
        uint256 balance = address(this).balance;
        // casting to 'uint128' is safe because ETH supply is less than `type(uint128).max`
        // forge-lint: disable-next-line(unsafe-typecast)
        _transferEther(inheritor, uint128(balance));
    }

    /* Router-driven state and funds management */

    function initialize(address _initializer, address _abiInterface, bool _isSmall) public onlyRouter {
        require(initializer == address(0), InitializerAlreadySet());

        require(!isSmall, IsSmallAlreadySet());

        StorageSlot.AddressSlot storage implementationSlot =
            StorageSlot.getAddressSlot(ERC1967Utils.IMPLEMENTATION_SLOT);

        require(implementationSlot.value == address(0), AbiInterfaceAlreadySet());

        initializer = _initializer;
        isSmall = _isSmall;
        implementationSlot.value = _abiInterface;
    }

    function performStateTransition(Gear.StateTransition calldata _transition)
        external
        payable
        onlyRouter
        returns (bytes32)
    {
        /// @dev Verify that the transition belongs to this contract.
        require(_transition.actorId == address(this), InvalidActorId());

        /// @dev Transfer value to router if valueToReceive is non-zero and has negative sign.
        if (_transition.valueToReceiveNegativeSign) {
            _retrievingEther(_transition.valueToReceive);
        }

        /// @dev Send all outgoing messages.
        bytes32 messagesHashesHash = _sendMessages(_transition.messages);

        /// @dev Send value for each claim.
        bytes32 valueClaimsHash = _claimValues(_transition.valueClaims);

        /// @dev Set inheritor if exited.
        if (_transition.exited) {
            _setInheritor(_transition.inheritor);
        } else {
            require(_transition.inheritor == address(0), InheritorMustBeZero());
        }

        /// @dev Update the state hash if changed.
        if (stateHash != _transition.newStateHash) {
            _updateStateHash(_transition.newStateHash);
        }

        /// @dev Return hash of performed state transition.
        return Gear.stateTransitionHash(
            _transition.actorId,
            _transition.newStateHash,
            _transition.exited,
            _transition.inheritor,
            _transition.valueToReceive,
            _transition.valueToReceiveNegativeSign,
            valueClaimsHash,
            messagesHashesHash
        );
    }

    function _sendMessage(bytes calldata _payload, bool _callReply)
        private
        onlyIfActive
        onlyAfterInitMessageOrInitializer
        returns (bytes32)
    {
        uint128 _value = uint128(msg.value);

        _retrievingEther(_value);

        uint256 _nonce = nonce;
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

    // TODO (breathx): consider when to emit event: on success in decoder, on failure etc.
    // TODO (breathx): make decoder gas configurable.
    // TODO (breathx): handle if goes to mailbox or not.
    function _sendMessages(Gear.Message[] calldata _messages) private returns (bytes32) {
        uint256 messagesLen = _messages.length;
        uint256 messagesHashesSize = messagesLen * 32;
        uint256 messagesHashesMemPtr = Memory.allocate(messagesHashesSize);
        uint256 offset = 0;

        for (uint256 i = 0; i < messagesLen; i++) {
            Gear.Message calldata message = _messages[i];

            // get the hash for this message
            bytes32 messageHash = Gear.messageHash(message);
            // store it at messagesHashes[offset : offset+32]
            Memory.writeWordAsBytes32(messagesHashesMemPtr, offset, messageHash);
            unchecked {
                offset += 32;
            }

            // send the message
            if (message.replyDetails.to == 0) {
                _sendMailboxedMessage(message);
            } else {
                _sendReplyMessage(message);
            }
        }

        return Hashes.efficientKeccak256AsBytes32(messagesHashesMemPtr, 0, messagesHashesSize);
    }

    /// @dev Value never sent since goes to mailbox.
    function _sendMailboxedMessage(Gear.Message calldata _message) private {
        if (!_tryParseAndEmitSailsEvent(_message)) {
            if (_message.call) {
                (bool success,) = _message.destination.call{gas: 500_000}(_message.payload);

                if (!success) {
                    /// @dev In case of failed call, we emit appropriate event to inform external users.
                    emit MessageCallFailed(_message.id, _message.destination, _message.value);

                    return;
                }
            }

            emit Message(_message.id, _message.destination, _message.payload, _message.value);
        }
    }

    /// @dev Tries to parse and emit Sails Event. Returns `true` in case of success and `false` in case of error.
    function _tryParseAndEmitSailsEvent(Gear.Message calldata _message) private returns (bool) {
        bytes calldata payload = _message.payload;

        // The format in which the Sails contract sends events is as follows:
        // - `uint8 topicsLength` (can be `1`, `2`, `3`, `4`).
        //    specifies which opcode (`log1`, `log2`, `log3`, `log4`) should be called.
        // - `bytes32 topic1` (required)
        //    should never match our event selectors!
        // - `bytes32 topic2` (optional)
        // - `bytes32 topic3` (optional)
        // - `bytes32 topic4` (optional)
        // - `bytes payload` (optional)
        //    contains encoded data of event in form of `abi.encode(...)`.
        if (!(_message.destination == ETH_EVENT_ADDR && _message.value == 0 && payload.length > 0)) {
            return false;
        }

        uint256 topicsLength;
        assembly ("memory-safe") {
            // `248` right bit shift is required to remove extra bits since `calldataload` returns `uint256`
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

        // we use offset 1 to skip `uint8 topicsLength`
        bytes32 topic1;
        assembly ("memory-safe") {
            topic1 := calldataload(add(payload.offset, 1))
        }

        /**
         * @dev SECURITY:
         *      Very important check because custom events can match our hashes!
         *      If we miss even 1 event that is emitted by Mirror, user will be able to fake protocol logic!
         */
        if (!(topic1 != StateChanged.selector && topic1 != MessageQueueingRequested.selector
                    && topic1 != ReplyQueueingRequested.selector && topic1 != ValueClaimingRequested.selector
                    && topic1 != OwnedBalanceTopUpRequested.selector
                    && topic1 != ExecutableBalanceTopUpRequested.selector && topic1 != Message.selector
                    && topic1 != Reply.selector && topic1 != ValueClaimed.selector)) {
            return false;
        }

        uint256 size;
        unchecked {
            size = payload.length - topicsLengthInBytes;
        }

        uint256 memPtr = Memory.allocate(size);
        assembly ("memory-safe") {
            calldatacopy(memPtr, add(payload.offset, topicsLengthInBytes), size)
        }

        // we use offset 1 to skip `uint8 topicsLength`
        // regular offsets: `32`, `64`, `96`
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

    /// @dev Non-zero value always sent since never goes to mailbox.
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
                _transferEther(_message.destination, _message.value);

                /// @dev In case of failed call, we emit appropriate event to inform external users.
                emit ReplyCallFailed(_message.value, _message.replyDetails.to, _message.replyDetails.code);
            }
        } else {
            _transferEther(_message.destination, _message.value);

            emit Reply(_message.payload, _message.value, _message.replyDetails.to, _message.replyDetails.code);
        }
    }

    // TODO (breathx): claimValues will fail if the program is exited: keep the funds on router.
    function _claimValues(Gear.ValueClaim[] calldata _claims) private returns (bytes32) {
        uint256 claimsLen = _claims.length;
        uint256 claimsHashesSize = claimsLen * 32;
        uint256 claimsHashesMemPtr = Memory.allocate(claimsHashesSize);
        uint256 offset = 0;

        for (uint256 i = 0; i < claimsLen; i++) {
            Gear.ValueClaim calldata claim = _claims[i];
            bytes32 claimHash = Gear.valueClaimHash(claim.messageId, claim.destination, claim.value);
            Memory.writeWordAsBytes32(claimsHashesMemPtr, offset, claimHash);
            unchecked {
                offset += 32;
            }

            _transferEther(claim.destination, claim.value);

            emit ValueClaimed(claim.messageId, claim.value);
        }

        return Hashes.efficientKeccak256AsBytes32(claimsHashesMemPtr, 0, claimsHashesSize);
    }

    // TODO (breathx): allow zero inheritor in router.
    function _setInheritor(address _inheritor) private onlyIfActive {
        /// @dev Set inheritor.
        exited = true;
        inheritor = _inheritor;

        /// @dev Transfer all available balance to the inheritor.
        transferLockedValueToInheritor();
    }

    function _updateStateHash(bytes32 _stateHash) private {
        /// @dev Set state hash.
        stateHash = _stateHash;

        /// @dev Emits an event signaling that the state has changed.
        emit StateChanged(stateHash);
    }

    /* Local helper functions */

    function _wvara(address routerAddr) private view returns (IWrappedVara) {
        address wvaraAddr = IRouter(routerAddr).wrappedVara();
        return IWrappedVara(wvaraAddr);
    }

    function _transferEther(address destination, uint128 value) private {
        if (value != 0) {
            (bool success,) = destination.call{value: value}("");
            require(success, EtherTransferToDestinationFailed());
        }
    }

    fallback() external payable {
        if (msg.value > 0 && msg.data.length == 0) {
            uint128 value = uint128(msg.value);

            emit OwnedBalanceTopUpRequested(value);
        } else if (!isSmall && msg.data.length >= 0x24) {
            // We only allow arbitrary calls to full mirror contracts, which are
            // more likely to come from their ERC1967 implementor.

            // The minimum call data length is 0x24 (36 bytes) because:
            // - 0x04 (4 bytes) for the function selector   [0x00..0x04)
            // - 0x20 (32 bytes) for the bool `callReply`   [0x04..0x24)

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
