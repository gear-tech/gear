// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {IMirror} from "src/IMirror.sol";
import {IDemoCallbacks} from "test/IDemoCallbacks.sol";

contract DemoCaller is IDemoCallbacks {
    IMirror public immutable VARA_ETH_PROGRAM;

    bool public replyOnMethodNameCalled;
    bool public onErrorReplyCalled;

    event MethodNameReplied(bytes32 messageId);

    event ErrorReplied(bytes32 messageId, bytes payload, bytes4 replyCode);

    error UnauthorizedCaller();

    constructor(IMirror _varaEthProgram) {
        VARA_ETH_PROGRAM = _varaEthProgram;
    }

    modifier onlyVaraEthProgram() {
        _onlyVaraEthProgram();
        _;
    }

    function _onlyVaraEthProgram() internal view {
        if (msg.sender != address(VARA_ETH_PROGRAM)) {
            revert UnauthorizedCaller();
        }
    }

    function methodName(bool isPanic) external returns (bytes32) {
        return VARA_ETH_PROGRAM.sendMessage(abi.encodePacked(isPanic), true);
    }

    /// forge-lint: disable-next-line(mixed-case-function)
    function replyOn_methodName(bytes32 messageId) external onlyVaraEthProgram {
        replyOnMethodNameCalled = true;

        emit MethodNameReplied(messageId);
    }

    function onErrorReply(bytes32 messageId, bytes calldata payload, bytes4 replyCode)
        external
        payable
        onlyVaraEthProgram
    {
        onErrorReplyCalled = true;

        emit ErrorReplied(messageId, payload, replyCode);
    }
}
