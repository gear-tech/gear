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

    error InsufficientValue(uint256 expected, uint256 actual);

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
}
