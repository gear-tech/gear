// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {Gear} from "./libraries/Gear.sol";

contract MirrorImpl is IMirror {
    bytes32 public stateHash;
    address public inheritor;
    /// @dev This nonce is the source for message ids unique generations.
    /// Must be bumped on each send.
    /// Zeroed nonce is always represent init message by eligible account.
    uint256 public nonce;
    address public immutable router;
    address public initializer;
    address public implAddress;

    function initialize(address, address, address, address) public pure {
        revert("must be called on MirrorAbi contract");
    }

    /* Primary Gear logic */

    function sendMessage(bytes calldata _payload, uint128 _value) external returns (bytes32) {
        bytes32 id = keccak256(abi.encodePacked(address(this), nonce++));

        emit MessageQueueingRequested(id, _source(), _payload, _value);

        return id;
    }

    function sendReply(bytes32 _repliedTo, bytes calldata _payload, uint128 _value) external {
        emit ReplyQueueingRequested(_repliedTo, _source(), _payload, _value);
    }

    function claimValue(bytes32 _claimedId) external {
        emit ValueClaimingRequested(_claimedId, _source());
    }

    function executableBalanceTopUp(uint128 _value) external {
        emit ExecutableBalanceTopUpRequested(_value);
    }

    function transferLockedValueToInheritor() public {
        uint256 balance = _wvara(router).balanceOf(address(this));
        _transferValue(inheritor, uint128(balance));
    }

    // NOTE (breathx): value to receive should be already handled in router.
    function performStateTransition(Gear.StateTransition calldata _transition) external returns (bytes32) {
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
    function _setInheritor(address _inheritor) private {
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
        return msg.sender;
    }

    function _transferValue(address destination, uint128 value) private {
        if (value != 0) {
            bool success = _wvara(router).transfer(destination, value);
            require(success, "failed to transfer WVara");
        }
    }
}
