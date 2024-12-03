// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {IMirrorProxy} from "./IMirrorProxy.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {IMirrorDecoder} from "./IMirrorDecoder.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";

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

    // TODO (breathx): sendMessage with msg.sender, but with tx.origin if decoder.
    function sendMessage(bytes calldata _payload, uint128 _value)
        external
        whileActive
        retrievingValue(_value)
        returns (bytes32)
    {
        // TODO (breathx): WITHIN THE PR check initializer.
        bytes32 id = keccak256(abi.encodePacked(address(this), nonce++));

        emit MessageQueueingRequested(id, _source(), _payload, _value);

        return id;
    }

    function sendReply(bytes32 _repliedTo, bytes calldata _payload, uint128 _value)
        external
        whileActive
        retrievingValue(_value)
    {
        emit ReplyQueueingRequested(_repliedTo, _source(), _payload, _value);
    }

    function claimValue(bytes32 _claimedId) external {
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

    function updateState(bytes32 newStateHash) external onlyRouter {
        if (stateHash != newStateHash) {
            stateHash = newStateHash;

            emit StateChanged(stateHash);
        }
    }

    function setInitializer(address _initializer) external onlyRouter {
        initializer = _initializer;
    }

    // TODO (breathx): handle after-all transfers to program on wvara event properly.
    function setInheritor(address _inheritor) external onlyRouter {
        inheritor = _inheritor;

        transferLockedValueToInheritor();
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
        _transferValue(destination, value);

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
        _transferValue(destination, value);

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
