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

    struct CreateProgramCall {
        bytes32 codeId;
        bytes32 salt;
        bytes initPayload;
        uint128 initValue;
        uint128 topUpValue;
    }

    error InsufficientValue(uint256 expected, uint256 actual);
    event SendMessageBatchResult(bytes32[] messageIds);

    receive() external payable {}

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
            MessageCall calldata item = calls[i];

            messageIds[i] = IMirror(item.mirror).sendMessage{value: item.value}(item.payload, false);
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
        returns (address[] memory programIds, bytes32[] memory messageIds)
    {
        programIds = new address[](calls.length);
        messageIds = new bytes32[](calls.length);

        IWrappedVara wvara = IWrappedVara(router.wrappedVara());

        uint256 consumed;

        for (uint256 i = 0; i < calls.length; ++i) {
            CreateProgramCall calldata item = calls[i];
            consumed += item.initValue;

            if (consumed > msg.value) {
                revert InsufficientValue(consumed, msg.value);
            }

            address programId = router.createProgram(item.codeId, item.salt, address(this));
            programIds[i] = programId;

            bytes32 messageId = IMirror(programId).sendMessage{value: item.initValue}(item.initPayload, false);
            messageIds[i] = messageId;

            if (item.topUpValue > 0) {
                require(wvara.transferFrom(msg.sender, address(this), item.topUpValue), "wVARA transferFrom failed");
                require(wvara.approve(programId, item.topUpValue), "wVARA approve failed");
                IMirror(programId).executableBalanceTopUp(item.topUpValue);
            }
        }

        if (consumed < msg.value) {
            (bool refunded,) = msg.sender.call{value: msg.value - consumed}("");
            require(refunded, "Refund failed");
        }
    }
}
