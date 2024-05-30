// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {IProgram} from "./IProgram.sol";

contract Router {
    address public owner;
    address public program;
    mapping(bytes32 => bool) public codeIds;

    struct CreateProgramData {
        bytes32 salt;
        bytes32 codeId;
        bytes32 stateHash;
    }

    struct UpdateProgramData {
        address program;
        bytes32 stateHash;
    }

    struct CommitData {
        bytes32[] codeIdsArray;
        CreateProgramData[] createProgramsArray;
        UpdateProgramData[] updateProgramsArray;
    }

    event UploadCode(address origin, bytes32 codeId, bytes32 blobTx);

    event UploadedCode(bytes32 codeId);

    event CreateProgram(address origin, bytes32 codeId, bytes salt, bytes initPayload, uint64 gasLimit, uint128 value);

    event CreatedProgram(address actorId);

    constructor() {
        owner = msg.sender;
    }

    function uploadCode(bytes32 codeId, bytes32 blobTx) external {
        emit UploadCode(tx.origin, codeId, blobTx);
    }

    function createProgram(
        bytes32 codeId,
        bytes calldata salt,
        bytes calldata initPayload,
        uint64 gasLimit,
        uint128 value
    ) external payable {
        require(codeIds[codeId], "unknown codeId");
        emit CreateProgram(tx.origin, codeId, salt, initPayload, gasLimit, value);
    }

    function setProgram(address _program) external {
        require(msg.sender == owner, "not owner");
        require(program == address(0), "program already set");
        program = _program;
    }

    function commit(CommitData calldata commitData) external {
        for (uint256 i = 0; i < commitData.codeIdsArray.length; i++) {
            bytes32 codeId = commitData.codeIdsArray[i];
            codeIds[codeId] = true;

            emit UploadedCode(codeId);
        }

        for (uint256 i = 0; i < commitData.createProgramsArray.length; i++) {
            CreateProgramData calldata data = commitData.createProgramsArray[i];
            require(codeIds[data.codeId], "unknown codeId");
            address actorId = Clones.cloneDeterministic(program, keccak256(abi.encodePacked(data.salt, data.codeId)));
            IProgram(actorId).setStateHash(data.stateHash);

            emit CreatedProgram(actorId);
        }

        for (uint256 i = 0; i < commitData.updateProgramsArray.length; i++) {
            UpdateProgramData calldata data = commitData.updateProgramsArray[i];
            IProgram(data.program).setStateHash(data.stateHash);
        }
    }
}
