// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {IMirrorProxy} from "./IMirrorProxy.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {IMirrorDecoder} from "./IMirrorDecoder.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";

// TODO: handle ETH sent in each contract.
contract Mirror is IMirror {
    bytes32 public stateHash;
    address public inheritor;
    // NOTE: Nonce 0 is used for init message in current implementation
    uint256 public nonce; /* = 1 */
    address public decoder;

    /* Operational functions */

    function router() public view returns (address) {
        return IMirrorProxy(address(this)).router();
    }

    /* Primary Gear logic */

    // TODO (breathx): sendMessage with msg.sender, but with tx.origin if decoder.
    function sendMessage(bytes calldata _payload, uint128 _value) external payable returns (bytes32) {
        require(inheritor == address(0), "program is terminated");

        uint128 baseFee = IRouter(router()).baseFee();
        _retrieveValueToRouter(baseFee + _value);

        bytes32 id = keccak256(abi.encodePacked(address(this), nonce++));

        emit MessageQueueingRequested(id, _source(), _payload, _value);

        return id;
    }

    function sendReply(bytes32 _repliedTo, bytes calldata _payload, uint128 _value) external payable {
        require(inheritor == address(0), "program is terminated");

        uint128 baseFee = IRouter(router()).baseFee();
        _retrieveValueToRouter(baseFee + _value);

        emit ReplyQueueingRequested(_repliedTo, _source(), _payload, _value);
    }

    function claimValue(bytes32 _claimedId) external {
        require(inheritor == address(0), "program is terminated");

        emit ValueClaimingRequested(_claimedId, _source());
    }

    function executableBalanceTopUp(uint128 _value) external payable {
        require(inheritor == address(0), "program is terminated");

        _retrieveValueToRouter(_value);

        emit ExecutableBalanceTopUpRequested(_value);
    }

    function sendValueToInheritor() public {
        require(inheritor != address(0), "program is not terminated");

        uint256 balance = IWrappedVara(IRouter(router()).wrappedVara()).balanceOf(address(this));
        _sendValueTo(inheritor, uint128(balance));
    }

    /* Router-driven state and funds management */

    function updateState(bytes32 newStateHash) external onlyRouter {
        if (stateHash != newStateHash) {
            stateHash = newStateHash;

            emit StateChanged(stateHash);
        }
    }

    // TODO (breathx): handle after-all transfers to program on wvara event properly.
    function setInheritor(address _inheritor) external onlyRouter {
        inheritor = _inheritor;

        sendValueToInheritor();
    }

    function messageSent(bytes32 id, address destination, bytes calldata payload, uint128 value) external onlyRouter {
        // TODO (breathx): handle if goes to mailbox or not. Send value in place or not.

        if (decoder != address(0)) {
            bytes memory callData =
                abi.encodeWithSelector(IMirrorDecoder.onMessageSent.selector, id, destination, payload, value);

            // Result is ignored here.
            // TODO (breathx): make gas configurable?
            (bool success,) = decoder.call{gas: 500_000}(callData);

            if (success) {
                // TODO (breathx): emit event with message hash?
                return;
            }
        }

        emit Message(id, destination, payload, value);
    }

    function replySent(address destination, bytes calldata payload, uint128 value, bytes32 replyTo, bytes4 replyCode)
        external
        onlyRouter
    {
        _sendValueTo(destination, value);

        if (decoder != address(0)) {
            bytes memory callData = abi.encodeWithSelector(
                IMirrorDecoder.onReplySent.selector, destination, payload, value, replyTo, replyCode
            );

            // Result is ignored here.
            // TODO (breathx): make gas configurable?
            (bool success,) = decoder.call{gas: 500_000}(callData);

            if (success) {
                // TODO (breathx): emit event with reply hash?
                return;
            }
        }

        emit Reply(payload, value, replyTo, replyCode);
    }

    function valueClaimed(bytes32 claimedId, address destination, uint128 value) external onlyRouter {
        _sendValueTo(destination, value);

        emit ValueClaimed(claimedId, value);
    }

    function createDecoder(address implementation, bytes32 salt) external onlyRouter {
        require(nonce == 0, "decoder could only be created before init message");
        require(decoder == address(0), "decoder could only be created once");

        decoder = Clones.cloneDeterministic(implementation, salt);

        IMirrorDecoder(decoder).initialize();
    }

    function initMessage(address source, bytes calldata payload, uint128 value, uint128 executableBalance)
        external
        onlyRouter
    {
        require(nonce == 0, "init message must be created before any others");

        /*
         * @dev: charging at this point is already made in router.
         */
        uint256 initNonce = nonce++;
        bytes32 id = keccak256(abi.encodePacked(address(this), initNonce));

        emit ExecutableBalanceTopUpRequested(executableBalance);
        emit MessageQueueingRequested(id, source, payload, value);
    }

    modifier onlyRouter() {
        require(msg.sender == router(), "only router contract is eligible for operation");
        _;
    }

    /* Local helper functions */

    function _source() private view returns (address) {
        if (msg.sender == decoder) {
            return tx.origin;
        } else {
            return msg.sender;
        }
    }

    function _retrieveValueToRouter(uint128 _value) private {
        address routerAddress = router();

        IWrappedVara wrappedVara = IWrappedVara(IRouter(routerAddress).wrappedVara());

        bool success = wrappedVara.transferFrom(_source(), routerAddress, _value);

        require(success, "failed to retrieve WVara");
    }

    function _sendValueTo(address destination, uint128 value) private {
        IWrappedVara wrappedVara = IWrappedVara(IRouter(router()).wrappedVara());

        if (value != 0) {
            bool success = wrappedVara.transfer(destination, value);

            require(success, "failed to send WVara");
        }
    }
}
