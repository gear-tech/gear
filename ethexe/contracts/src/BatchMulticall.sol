// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.33;

import {IMirror} from "src/IMirror.sol";
import {IRouter} from "src/IRouter.sol";
import {IWrappedVara} from "src/IWrappedVara.sol";

/**
 * @dev BatchMulticall smart contract is responsible for batching multiple calls to Mirror smart contracts.
 *      This is useful for reducing number of transactions when interacting with multiple Mirror contracts.
 *      Mostly used in crate [`ethexe-node-loader`](ethexe/node-loader), which is responsible for testing our network.
 *      Since we use `anvil` as Ethereum node, this contract allows us to avoid waiting for block time and load node much faster.
 *      This contract allows both batching of messages and batching of program creations.
 *      Furthermore, when creating programs, it offers full flow:
 *      - approval of WVARA ERC20 token for created program (Mirror)
 *      - top-up of executable balance for created program in WVARA ERC20 token (Mirror)
 *      - sending initial message to created program (Mirror)
 *      All of these actions are done in one transaction, which is much faster than doing them separately.
 */
contract BatchMulticall {
    /**
     * @dev There is not enough value sent with transaction to cover calls.
     */
    error InsufficientValue(uint256 expected, uint256 actual);

    /**
     * @dev Refunding excess value to sender failed.
     */
    error RefundFailed();

    /**
     * @dev Transferring WVARA token from sender to this contract failed.
     */
    error TransferFromFailed();

    /**
     * @dev Approving WVARA token for created program (Mirror) failed.
     */
    error ApproveFailed();

    /**
     * @dev Emitted when batch of messages is sent. It contains array of message ids that were sent.
     */
    event SendMessageBatchResult(bytes32[] messageIds);

    /**
     * @dev Represents call to send message through Mirror contract.
     *      It will be sent through `IMirror(mirror).sendMessage{value: value}(payload, false)`.
     *      (`callReply` is always `false` since we don't want to call reply hook).
     */
    struct MessageCall {
        address mirror;
        bytes payload;
        uint128 value;
    }

    /**
     * @dev Represents call to create Mirror through Router contract.
     *      It will be sent through `IRouter(router).createProgram(codeId, salt, address(this))`,
     *      where `overrideInitializer` is always `address(this)` since we want to send initial message from this contract.
     *      Then, if `topUpValue` is greater than 0, it will approve WVARA token and top up executable balance for created Mirror.
     *      Finally, it will send initial message to created Mirror through `IMirror(programId).sendMessage{value: initValue}(initPayload, false)`.
     *     (`callReply` is always `false` since we don't want to call reply hook).
     */
    struct CreateProgramCall {
        bytes32 codeId;
        bytes32 salt;
        bytes initPayload;
        uint128 initValue;
        uint128 topUpValue;
    }

    /**
     * @dev Sends batch of messages through Mirror contracts.
     * @param calls Array of `MessageCall` structs representing calls to send messages through Mirror contracts.
     */
    function sendMessageBatch(MessageCall[] calldata calls) external payable {
        bytes32[] memory messageIds = new bytes32[](calls.length);

        uint256 consumed;

        for (uint256 i = 0; i < calls.length; ++i) {
            consumed += calls[i].value;
        }

        require(consumed <= msg.value, InsufficientValue(consumed, msg.value));

        for (uint256 i = 0; i < calls.length; ++i) {
            MessageCall calldata messageCall = calls[i];

            messageIds[i] =
                IMirror(messageCall.mirror).sendMessage{value: messageCall.value}(messageCall.payload, false);
        }

        if (consumed < msg.value) {
            (bool success,) = msg.sender.call{value: msg.value - consumed}("");
            require(success, RefundFailed());
        }

        emit SendMessageBatchResult(messageIds);
    }

    /**
     * @dev Creates batch of programs through Router contract and sends initial messages to them.
     * @param router The Router contract address.
     * @param calls Array of `CreateProgramCall` structs representing calls to create programs through Router contract.
     * @return programIds Array of created program IDs.
     * @return messageIds Array of message IDs for the initial messages sent to each created program.
     */
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

            require(consumed <= msg.value, InsufficientValue(consumed, msg.value));

            address programId = router.createProgram(createProgramCall.codeId, createProgramCall.salt, address(this));
            programIds[i] = programId;
            IMirror mirror = IMirror(programId);

            if (createProgramCall.topUpValue > 0) {
                require(
                    wvara.transferFrom(msg.sender, address(this), createProgramCall.topUpValue), TransferFromFailed()
                );
                require(wvara.approve(programId, createProgramCall.topUpValue), ApproveFailed());
                mirror.executableBalanceTopUp(createProgramCall.topUpValue);
            }

            bytes32 messageId =
                mirror.sendMessage{value: createProgramCall.initValue}(createProgramCall.initPayload, false);
            messageIds[i] = messageId;
        }

        if (consumed < msg.value) {
            (bool success,) = msg.sender.call{value: msg.value - consumed}("");
            require(success, RefundFailed());
        }

        return (programIds, messageIds);
    }

    /**
     * @dev Fallback function to receive Ether.
     *      This is necessary because `function _transferEther(address destination, uint128 value)` in `Mirror`
     *      will send `value` (ETH) to address of `BatchMulticall` smart contract
     *      (since in context of call `IMirror(messageCall.mirror).sendMessage(...)`: `msg.sender = address(BatchMulticall)`)
     */
    receive() external payable {}
}
