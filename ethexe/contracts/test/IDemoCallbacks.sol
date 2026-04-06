// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {ICallbacks} from "src/ICallbacks.sol";

interface IDemoCallbacks is ICallbacks {
    /// forge-lint: disable-next-line(mixed-case-function)
    function replyOn_methodName(bytes32 messageId) external;
}
