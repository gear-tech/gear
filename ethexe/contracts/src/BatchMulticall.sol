// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract BatchMulticall {
    error InsufficientValue(uint256 expected, uint256 actual);

    event SendMessageBatchResult(bytes32[] messageIds);

    struct MessageCall {
        address mirror;
        bytes payload;
        uint128 value;
    }

    struct CreateProgramCall {
        bytes32 codeId;
        bytes32 salt;
        bytes initPayload;
        uint128 initValue;
        uint128 topUpValue;
    }

    function sendMessageBatch(MessageCall[] calldata calls) external payable {
        bytes32[] memory messageIds = new bytes32[](calls.length);

        uint256 consumed;

        for (uint256 i = 0; i < calls.length; ++i) {
            consumed += calls[i].value;
        }

        if (consumed > msg.value) {
            revert InsufficientValue(consumed, msg.value);
        }

        for (uint256 i = 0; i < calls.length; ++i) {
            MessageCall calldata messageCall = calls[i];

            messageIds[i] =
                IMirror(messageCall.mirror).sendMessage{value: messageCall.value}(messageCall.payload, false);
        }

        if (consumed < msg.value) {
            (bool refunded,) = msg.sender.call{value: msg.value - consumed}("");
            require(refunded, "Refund failed");
        }

        emit SendMessageBatchResult(messageIds);
    }

    function createProgramBatch(IRouter router, CreateProgramCall[] calldata calls)
        external
        payable
        returns (address[] memory, bytes32[] memory)
    {
        address[] memory programIds = new address[](calls.length);
        bytes32[] memory messageIds = new bytes32[](calls.length);

        IWrappedVara wvara = IWrappedVara(router.wrappedVara());

        uint256 consumed;

        for (uint256 i = 0; i < calls.length; ++i) {
            CreateProgramCall calldata createProgramCall = calls[i];
            consumed += createProgramCall.initValue;

            if (consumed > msg.value) {
                revert InsufficientValue(consumed, msg.value);
            }

            address programId = router.createProgram(createProgramCall.codeId, createProgramCall.salt, address(this));
            programIds[i] = programId;
            IMirror mirror = IMirror(programId);

            if (createProgramCall.topUpValue > 0) {
                require(
                    wvara.transferFrom(msg.sender, address(this), createProgramCall.topUpValue),
                    "wVARA transferFrom failed"
                );
                require(wvara.approve(programId, createProgramCall.topUpValue), "wVARA approve failed");
                mirror.executableBalanceTopUp(createProgramCall.topUpValue);
            }

            bytes32 messageId =
                mirror.sendMessage{value: createProgramCall.initValue}(createProgramCall.initPayload, false);
            messageIds[i] = messageId;
        }

        if (consumed < msg.value) {
            (bool refunded,) = msg.sender.call{value: msg.value - consumed}("");
            require(refunded, "Refund failed");
        }

        return (programIds, messageIds);
    }

    receive() external payable {}
}
