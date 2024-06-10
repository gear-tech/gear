// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

interface IRouter {
    function COUNT_OF_VALIDATORS() external view returns (uint256);
    function REQUIRED_SIGNATURES() external view returns (uint256);
    function WRAPPED_VARA() external view returns (address);

    function program() external view returns (address);
    function countOfValidators() external view returns (uint256);
    // TODO: support mappings: validators, codeIds, programs

    function setProgram(address _program) external;

    function addValidators(address[] calldata validatorsArray) external;

    function removeValidators(address[] calldata validatorsArray) external;

    function uploadCode(bytes32 codeId, bytes32 blobTx) external;

    function createProgram(bytes32 codeId, bytes32 salt, bytes calldata initPayload, uint64 gasLimit)
        external
        payable;

    function sendMessage(address destination, bytes calldata payload, uint64 gasLimit, uint128 value) external;

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit, uint128 value) external;

    function claimValue(bytes32 messageId) external;
}
