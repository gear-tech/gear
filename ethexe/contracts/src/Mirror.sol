// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";
import {ERC1967Utils} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Utils.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {Gear} from "./libraries/Gear.sol";

contract Mirror is IMirror {
    address internal constant ETH_EVENT_ADDR = 0xFFfFfFffFFfffFFfFFfFFFFFffFFFffffFfFFFfF;

    address public immutable router;

    address public inheritor;
    /// @dev This nonce is the source for message ids unique generations. Must be bumped on each send. Zeroed nonce is always represent init message by eligible account.
    address public initializer;
    bytes32 public stateHash;
    uint256 public nonce;

    constructor(address _router) {
        router = _router;
    }

    /// @dev Only the router can call functions marked with this modifier.
    modifier onlyRouter() {
        require(msg.sender == router, "caller is not the router");
        _;
    }

    /// @dev Non-zero value must be transferred from source to router in functions marked with this modifier.
    modifier retrievingValue(uint128 value) {
        if (value != 0) {
            bool success = _wvara(router).transferFrom(msg.sender, router, value);
            require(success, "failed to transfer non-zero amount of WVara from source to router");
        }
        _;
    }

    // TODO (breathx): terminated programs compute threshold must always be treated as balance-enough.
    /// @dev Functions marked with this modifier can be called only after the program is terminated.
    modifier whenTerminated() {
        require(inheritor != address(0), "program is not terminated");
        _;
    }

    /// @dev Functions marked with this modifier can be called only after the initializer has created the init message.
    modifier whenInitMessageCreated() {
        require(nonce > 0, "initializer hasn't created init message yet");
        _;
    }

    /// @dev Functions marked with this modifier can be called only after the initializer has created the init message or from the initializer (first access).
    modifier whenInitMessageCreatedOrFromInitializer() {
        require(
            nonce > 0 || msg.sender == initializer,
            "initializer hasn't created init message yet; and source is not initializer"
        );
        _;
    }

    /// @dev Functions marked with this modifier can be called only if the program is active.
    modifier whileActive() {
        require(inheritor == address(0), "program is terminated");
        _;
    }

    /* Primary Gear logic */

    function sendMessage(bytes calldata _payload, uint128 _value)
        public
        whileActive
        whenInitMessageCreatedOrFromInitializer
        retrievingValue(_value)
        returns (bytes32)
    {
        bytes32 id = keccak256(abi.encodePacked(address(this), nonce++));

        emit MessageQueueingRequested(id, msg.sender, _payload, _value);

        return id;
    }

    function sendReply(bytes32 _repliedTo, bytes calldata _payload, uint128 _value)
        external
        whileActive
        whenInitMessageCreated
        retrievingValue(_value)
    {
        emit ReplyQueueingRequested(_repliedTo, msg.sender, _payload, _value);
    }

    function claimValue(bytes32 _claimedId) external whenInitMessageCreated {
        emit ValueClaimingRequested(_claimedId, msg.sender);
    }

    function executableBalanceTopUp(uint128 _value) external whileActive retrievingValue(_value) {
        emit ExecutableBalanceTopUpRequested(_value);
    }

    function transferLockedValueToInheritor() public whenTerminated {
        uint256 balance = _wvara(router).balanceOf(address(this));
        _transferValue(inheritor, uint128(balance));
    }

    /* Router-driven state and funds management */

    function initialize(address _initializer, address _abiInterface) public onlyRouter {
        require(initializer == address(0), "initializer could only be set once");
        StorageSlot.AddressSlot storage implementationSlot =
            StorageSlot.getAddressSlot(ERC1967Utils.IMPLEMENTATION_SLOT);
        require(implementationSlot.value == address(0), "abi interface could only be set once");

        initializer = _initializer;
        if (_abiInterface != address(0)) {
            implementationSlot.value = _abiInterface;
        }
    }

    // NOTE (breathx): value to receive should be already handled in router.
    function performStateTransition(Gear.StateTransition calldata _transition) external onlyRouter returns (bytes32) {
        /// @dev Verify that the transition belongs to this contract.
        require(_transition.actorId == address(this), "actorId must be this contract");

        /// @dev Send all outgoing messages.
        bytes32 messagesHashesHash = _sendMessages(_transition.messages);

        /// @dev Send value for each claim.
        bytes32 valueClaimsHash = _claimValues(_transition.valueClaims);

        /// @dev Set inheritor if specified.
        if (_transition.inheritor != address(0)) {
            _setInheritor(_transition.inheritor);
        }

        /// @dev Update the state hash if changed.
        if (stateHash != _transition.newStateHash) {
            _updateStateHash(_transition.newStateHash);
        }

        /// @dev Return hash of performed state transition.
        return Gear.stateTransitionHash(
            _transition.actorId,
            _transition.newStateHash,
            _transition.inheritor,
            _transition.valueToReceive,
            valueClaimsHash,
            messagesHashesHash
        );
    }

    // TODO (breathx): consider when to emit event: on success in decoder, on failure etc.
    // TODO (breathx): make decoder gas configurable.
    // TODO (breathx): handle if goes to mailbox or not.
    function _sendMessages(Gear.Message[] calldata _messages) private returns (bytes32) {
        bytes memory messagesHashes;

        for (uint256 i = 0; i < _messages.length; i++) {
            Gear.Message calldata message = _messages[i];

            messagesHashes = bytes.concat(messagesHashes, Gear.messageHash(message));

            // TODO (breathx): optimize it to bytes WITHIN THE PR.
            if (message.replyDetails.to == 0) {
                _sendMailboxedMessage(message);
            } else {
                _sendReplyMessage(message);
            }
        }

        return keccak256(messagesHashes);
    }

    /// @dev Value never sent since goes to mailbox.
    function _sendMailboxedMessage(Gear.Message calldata _message) private {
        bytes calldata payload = _message.payload;

        if (_message.destination == ETH_EVENT_ADDR && _message.value == 0 && payload.length > 0) {
            uint256 topicsLength;
            assembly ("memory-safe") {
                topicsLength := shr(248, calldataload(payload.offset))
            }

            if (topicsLength >= 1 && topicsLength <= 4) {
                uint256 topicsLengthInBytes;
                unchecked {
                    topicsLengthInBytes = 1 + topicsLength * 32;
                }

                if (payload.length >= topicsLengthInBytes) {
                    bytes32 topic1;
                    assembly ("memory-safe") {
                        topic1 := calldataload(add(payload.offset, 1))
                    }

                    if (
                        topic1 != StateChanged.selector && topic1 != MessageQueueingRequested.selector
                            && topic1 != ReplyQueueingRequested.selector && topic1 != ValueClaimingRequested.selector
                            && topic1 != ExecutableBalanceTopUpRequested.selector && topic1 != Message.selector
                            && topic1 != Reply.selector && topic1 != ValueClaimed.selector
                    ) {
                        uint256 size;
                        unchecked {
                            size = payload.length - topicsLengthInBytes;
                        }

                        uint256 memPtr = Memory.allocate(size);
                        assembly ("memory-safe") {
                            calldatacopy(memPtr, add(payload.offset, topicsLengthInBytes), size)
                        }

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

                        return;
                    }
                }
            }
        }

        emit Message(_message.id, _message.destination, _message.payload, _message.value);
    }

    /// @dev Non-zero value always sent since never goes to mailbox.
    function _sendReplyMessage(Gear.Message calldata _message) private {
        _transferValue(_message.destination, _message.value);

        emit Reply(_message.payload, _message.value, _message.replyDetails.to, _message.replyDetails.code);
    }

    function _claimValues(Gear.ValueClaim[] calldata _claims) private returns (bytes32) {
        bytes memory valueClaimsBytes;

        for (uint256 i = 0; i < _claims.length; i++) {
            Gear.ValueClaim calldata claim = _claims[i];

            valueClaimsBytes = bytes.concat(valueClaimsBytes, Gear.valueClaimBytes(claim));

            _transferValue(claim.destination, claim.value);

            emit ValueClaimed(claim.messageId, claim.value);
        }

        return keccak256(valueClaimsBytes);
    }

    // TODO (breathx): optimize inheritor to bytes WITHIN THE PR.
    function _setInheritor(address _inheritor) private whileActive {
        /// @dev Set inheritor.
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

    function _transferValue(address destination, uint128 value) private {
        if (value != 0) {
            bool success = _wvara(router).transfer(destination, value);
            require(success, "failed to transfer WVara");
        }
    }

    fallback() external payable {
        StorageSlot.AddressSlot storage implementationSlot =
            StorageSlot.getAddressSlot(ERC1967Utils.IMPLEMENTATION_SLOT);
        address _abiInterface = implementationSlot.value;

        if (_abiInterface != address(0)) {
            require(msg.data.length >= 0x24);

            uint256 value;
            assembly ("memory-safe") {
                value := calldataload(0x04)
            }

            bytes32 messageId = sendMessage(msg.data, uint128(value));

            assembly ("memory-safe") {
                mstore(0x00, messageId)
                return(0x00, 0x20)
            }
        } else {
            revert();
        }
    }
}
