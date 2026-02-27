// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract BatchMulticall {
    struct MessageCall {
        address mirror;
        bytes payload;
        uint128 value;
    }

    struct ProgramInitCall {
        bytes32 codeId;
        bytes32 salt;
        bytes payload;
        uint128 topUpValue;
        uint128 initValue;
    }

    error InsufficientValue(uint256 expected, uint256 actual);
    error Forbidden();
    error ApproveFailed();

    event ProgramCreatedAndInitialized(uint256 indexed callId, address indexed programId, bytes32 messageId);
    event ProgramCreateAndInitFailed(uint256 indexed callId, bytes reason);

    function sendMessageBatch(
        MessageCall[] calldata calls
    ) external payable returns (bool[] memory success, bytes32[] memory messageIds) {
        success = new bool[](calls.length);
        messageIds = new bytes32[](calls.length);

        uint256 consumed;

        for (uint256 i = 0; i < calls.length; ++i) {
            MessageCall calldata item = calls[i];
            consumed += item.value;

            if (consumed > msg.value) {
                revert InsufficientValue(consumed, msg.value);
            }

            try IMirror(item.mirror).sendMessage{value: item.value}(item.payload, false) returns (
                bytes32 messageId
            ) {
                success[i] = true;
                messageIds[i] = messageId;
            } catch {
                success[i] = false;
            }
        }

        if (consumed < msg.value) {
            (bool refunded, ) = payable(msg.sender).call{value: msg.value - consumed}("");
            require(refunded, "Refund failed");
        }
    }

    function createProgramApproveTopUpAndInitBatch(
        address router,
        address wrappedVara,
        ProgramInitCall[] calldata calls
    ) external payable returns (bool[] memory success, address[] memory programIds, bytes32[] memory messageIds) {
        success = new bool[](calls.length);
        programIds = new address[](calls.length);
        messageIds = new bytes32[](calls.length);

        uint256 consumed;

        for (uint256 i = 0; i < calls.length; ++i) {
            ProgramInitCall calldata item = calls[i];
            consumed += item.initValue;

            if (consumed > msg.value) {
                revert InsufficientValue(consumed, msg.value);
            }

            try
                this.createProgramApproveTopUpAndInit{value: item.initValue}(router, wrappedVara, item)
            returns (address programId, bytes32 messageId) {
                success[i] = true;
                programIds[i] = programId;
                messageIds[i] = messageId;
                emit ProgramCreatedAndInitialized(i, programId, messageId);
            } catch (bytes memory reason) {
                success[i] = false;
                emit ProgramCreateAndInitFailed(i, reason);
            }
        }

        if (consumed < msg.value) {
            (bool refunded, ) = payable(msg.sender).call{value: msg.value - consumed}("");
            require(refunded, "Refund failed");
        }
    }

    function createProgramApproveTopUpAndInit(
        address router,
        address wrappedVara,
        ProgramInitCall calldata call
    ) external payable returns (address programId, bytes32 messageId) {
        if (msg.sender != address(this)) {
            revert Forbidden();
        }

        programId = IRouter(router).createProgram(call.codeId, call.salt, address(0));

        bool approved = IWrappedVara(wrappedVara).approve(programId, call.topUpValue);
        if (!approved) {
            revert ApproveFailed();
        }

        IMirror(programId).executableBalanceTopUp(call.topUpValue);
        messageId = IMirror(programId).sendMessage{value: call.initValue}(call.payload, false);
    }
}
