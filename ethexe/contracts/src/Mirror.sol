// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {IMirrorProxy} from "./IMirrorProxy.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

// TODO: handle ETH sent in each contract.
contract Mirror is IMirror {
    bytes32 public stateHash;
    // NOTE: Nonce 0 is used for init message in current implementation
    uint256 public nonce = 1;

    /* Operational functions */

    function router() public view returns (address) {
        return IMirrorProxy(address(this)).router();
    }

    /* Primary Gear logic */

    function sendMessage(bytes calldata _payload, uint128 _value) external payable returns (bytes32) {
        uint128 baseFee = IRouter(router()).baseFee();
        _retreiveValueToRouter(baseFee + _value);

        bytes32 id = keccak256(abi.encodePacked(address(this), nonce++));

        emit MessageQueued(id, tx.origin, _payload, _value);

        return id;
    }

    function sendReply(bytes32 _repliedTo, bytes calldata _payload, uint128 _value) external payable {
        uint128 baseFee = IRouter(router()).baseFee();
        _retreiveValueToRouter(baseFee + _value);

        emit ReplyQueueingRequested(_repliedTo, tx.origin, _payload, _value);
    }

    function claimValue(bytes32 _claimedId) external {
        // TODO: should we charge here something for try?
        emit ClaimValueRequested(_claimedId, tx.origin);
    }

    function executableBalanceTopUp(uint128 _value) external payable {
        _retreiveValueToRouter(_value);

        emit ExecutableBalanceTopUpRequested(_value);
    }

    /* Router-driven state and funds management */

    function updateState(bytes32 prevStateHash, bytes32 newStateHash) external onlyRouter {
        require(stateHash == prevStateHash, "invalid transition initial state hash");

        if (prevStateHash != newStateHash) {
            stateHash = newStateHash;

            emit StateChanged(stateHash);
        }
    }

    function messageSent(bytes32 id, address destination, bytes calldata payload, uint128 value) external onlyRouter {
        // TODO (breathx): handle if goes to mailbox or not. Send value in place or not.
        emit Message(id, destination, payload, value);
    }

    function replySent(address destination, bytes calldata payload, uint128 value, bytes32 replyTo, bytes4 replyCode)
        external
        onlyRouter
    {
        _sendValueTo(destination, value);

        emit Reply(payload, value, replyTo, replyCode);
    }

    function valueClaimed(bytes32 claimedId, address destination, uint128 value) external onlyRouter {
        _sendValueTo(destination, value);

        emit ValueClaimed(claimedId, value);
    }

    function executableBalanceBurned(uint128 value) external onlyRouter {
        _sendValueTo(router(), value);
    }

    function initMessage(address source, bytes calldata payload, uint128 value, uint128 executableBalance)
        external
        onlyRouter
    {
        // @dev: charging at this point already made on router side.
        uint256 initNonce = 0;
        bytes32 id = keccak256(abi.encodePacked(address(this), initNonce));

        emit ExecutableBalanceTopUpRequested(executableBalance);
        emit MessageQueued(id, source, payload, value);
    }

    modifier onlyRouter() {
        require(msg.sender == router(), "only router contract is eligible for operation");
        _;
    }

    /* Local helper functions */

    function _retreiveValueToRouter(uint128 _value) public {
        address routerAddress = router();

        IWrappedVara wrappedVara = IWrappedVara(IRouter(routerAddress).wrappedVara());

        bool success = wrappedVara.transferFrom(tx.origin, routerAddress, _value);

        require(success, "failed to retreive WVara");
    }

    // TODO (breathx): for such public fns should there be modifier? (cc) StackOverflowException
    function _sendValueTo(address destination, uint128 value) public {
        IWrappedVara wrappedVara = IWrappedVara(IRouter(router()).wrappedVara());

        if (value != 0) {
            bool success = wrappedVara.transfer(destination, value);

            require(success, "failed to send WVara");
        }
    }
}
