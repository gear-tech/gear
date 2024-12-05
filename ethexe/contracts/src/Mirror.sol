// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {IMirrorProxy} from "./IMirrorProxy.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {IMirrorDecoder} from "./IMirrorDecoder.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {Gear} from "./libraries/Gear.sol";

contract Mirror is IMirror {
    address public decoder;
    address public inheritor;
    /// @dev This nonce is the source for message ids unique generations. Must be bumped on each send. Zeroed nonce is always represent init message by eligible account.
    address public initializer;
    bytes32 public stateHash;
    uint256 public nonce;

    /// @dev Only the router can call functions marked with this modifier.
    modifier onlyRouter() {
        require(msg.sender == router(), "caller is not the router");
        _;
    }

    /// @dev Non-zero value must be transferred from source to router in functions marked with this modifier.
    modifier retrievingValue(uint128 value) {
        if (value != 0) {
            address routerAddr = router();
            bool success = _wvara(routerAddr).transferFrom(_source(), routerAddr, value);
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
            nonce > 0 || _source() == initializer,
            "initializer hasn't created init message yet; and source is not initializer"
        );
        _;
    }

    /// @dev Functions marked with this modifier can be called only if the program is active.
    modifier whileActive() {
        require(inheritor == address(0), "program is terminated");
        _;
    }

    /* Operational functions */

    function router() public view returns (address) {
        return IMirrorProxy(address(this)).router();
    }

    /* Primary Gear logic */

    function sendMessage(bytes calldata _payload, uint128 _value)
        external
        whileActive
        whenInitMessageCreatedOrFromInitializer
        retrievingValue(_value)
        returns (bytes32)
    {
        bytes32 id = keccak256(abi.encodePacked(address(this), nonce++));

        emit MessageQueueingRequested(id, _source(), _payload, _value);

        return id;
    }

    function sendReply(bytes32 _repliedTo, bytes calldata _payload, uint128 _value)
        external
        whileActive
        whenInitMessageCreated
        retrievingValue(_value)
    {
        emit ReplyQueueingRequested(_repliedTo, _source(), _payload, _value);
    }

    function claimValue(bytes32 _claimedId) external whenInitMessageCreated {
        emit ValueClaimingRequested(_claimedId, _source());
    }

    function executableBalanceTopUp(uint128 _value) external whileActive retrievingValue(_value) {
        emit ExecutableBalanceTopUpRequested(_value);
    }

    function transferLockedValueToInheritor() public whenTerminated {
        uint256 balance = _wvara(router()).balanceOf(address(this));
        _transferValue(inheritor, uint128(balance));
    }

    /* Router-driven state and funds management */

    function initialize(address _initializer, address _decoder) public onlyRouter {
        require(initializer == address(0), "initializer could only be set once");
        require(decoder == address(0), "initializer could only be set once");

        initializer = _initializer;
        decoder = _decoder;
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
        if (_transition.inheritor.length != 0) {
            _setInheritor(Gear.decodePackedAddress(_transition.inheritor));
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

            if (message.replyDetails.length == 0) {
                _sendMailboxedMessage(message);
            } else {
                _sendReplyMessage(message);
            }
        }

        return keccak256(messagesHashes);
    }

    /// @dev Value never sent since goes to mailbox.
    function _sendMailboxedMessage(Gear.Message calldata _message) private {
        if (decoder != address(0)) {
            bytes memory callData = abi.encodeWithSelector(
                IMirrorDecoder.onMessageSent.selector,
                _message.id,
                _message.destination,
                _message.payload,
                _message.value
            );

            // Result is ignored here.
            (bool success,) = decoder.call{gas: 500_000}(callData);

            if (success) {
                return;
            }
        }

        emit Message(_message.id, _message.destination, _message.payload, _message.value);
    }

    /// @dev Non-zero value always sent since never goes to mailbox.
    function _sendReplyMessage(Gear.Message calldata _message) private {
        Gear.ReplyDetails memory replyDetails = Gear.decodeReplyDetails(_message.replyDetails);

        _transferValue(_message.destination, _message.value);

        if (decoder != address(0)) {
            bytes memory callData = abi.encodeWithSelector(
                IMirrorDecoder.onReplySent.selector,
                _message.destination,
                _message.payload,
                _message.value,
                replyDetails.to,
                replyDetails.code
            );

            // Result is ignored here.
            (bool success,) = decoder.call{gas: 500_000}(callData);

            if (success) {
                return;
            }
        }

        emit Reply(_message.payload, _message.value, replyDetails.to, replyDetails.code);
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

    function _source() private view returns (address) {
        if (msg.sender == decoder) {
            return tx.origin;
        } else {
            return msg.sender;
        }
    }

    function _transferValue(address destination, uint128 value) private {
        if (value != 0) {
            bool success = _wvara(router()).transfer(destination, value);
            require(success, "failed to transfer WVara");
        }
    }
}
