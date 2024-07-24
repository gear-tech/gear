// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Test, console} from "forge-std/Test.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {IMirror, Mirror} from "../src/Mirror.sol";
import {MirrorProxy} from "../src/MirrorProxy.sol";
import {IRouter, Router} from "../src/Router.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

contract RouterTest is Test {
    using MessageHashUtils for address;

    address public deployerAddress;

    address[] public validatorsArray;
    uint256[] public validatorsPrivateKeys;

    WrappedVara public wrappedVara;
    Router public router;
    Mirror public mirror;
    MirrorProxy public mirrorProxy;

    function setUp() public {
        // deployerAddress = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;

        // validatorsArray.push(0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1);
        // validatorsPrivateKeys.push(0x32816f851b9cc71c4eb956214ded8cf481f7af66c125d1fb9deae366ae4f13a6);

        // startPrank(deployerAddress);

        // wrappedVara = WrappedVara(
        //     Upgrades.deployTransparentProxy(
        //         "WrappedVara.sol", deployerAddress, abi.encodeCall(WrappedVara.initialize, (deployerAddress, 6))
        //     )
        // );

        // address programAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 2);
        // address minimalProgramAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 3);
        // address wrappedVaraAddress = address(wrappedVara);

        // router = Router(
        //     Upgrades.deployTransparentProxy(
        //         "Router.sol",
        //         deployerAddress,
        //         abi.encodeCall(
        //             Router.initialize,
        //             (deployerAddress, programAddress, minimalProgramAddress, wrappedVaraAddress, validatorsArray)
        //         )
        //     )
        // );
        // program = new Program();
        // minimalProgram = new MinimalProgram(address(router));

        // wrappedVara.approve(address(router), type(uint256).max);

        // assertEq(router.program(), address(program));
        // assertEq(router.minimalProgram(), address(minimalProgram));
        // assertEq(minimalProgram.router(), address(router));
    }

    function test_DemoPing() public {
        // startPrank(deployerAddress);

        // bytes32 codeId = bytes32(uint256(1));
        // bytes32 blobTx = bytes32(uint256(2));

        // router.uploadCode(codeId, blobTx);

        // IRouter.CodeCommitment[] memory codeCommitmentsArray = new IRouter.CodeCommitment[](1);
        // codeCommitmentsArray[0] = IRouter.CodeCommitment(codeId, true);

        // commitCodes(codeCommitmentsArray);

        // address actorId = router.createProgram(codeId, "salt", "PING", 1_000_000_000);
        // IProgram deployedProgram = IProgram(actorId);

        // assertEq(deployedProgram.router(), address(router));
        // assertEq(deployedProgram.stateHash(), 0);

        // vm.roll(100);

        // IRouter.OutgoingMessage[] memory outgoingMessages = new IRouter.OutgoingMessage[](1);
        // outgoingMessages[0] = IRouter.OutgoingMessage(0, deployerAddress, "PONG", 0, IRouter.ReplyDetails(0, 0));

        // IRouter.StateTransition[] memory transitionsArray = new IRouter.StateTransition[](1);
        // IRouter.BlockCommitment[] memory blockCommitmentsArray = new IRouter.BlockCommitment[](1);
        // transitionsArray[0] = IRouter.StateTransition(actorId, 0, bytes32(uint256(1)), outgoingMessages);
        // blockCommitmentsArray[0] = IRouter.BlockCommitment(
        //     bytes32(uint256(1)), bytes32(uint256(0)), blockhash(block.number - 1), transitionsArray
        // );

        // commitBlocks(blockCommitmentsArray);

        // assertEq(deployedProgram.stateHash(), bytes32(uint256(1)));
    }

    function startPrank(address msgSender) private {
        // vm.startPrank(msgSender, msgSender);
    }

    function commitCodes(IRouter.CodeCommitment[] memory codeCommitmentsArray) private {
        // bytes memory codesBytes;

        // for (uint256 i = 0; i < codeCommitmentsArray.length; i++) {
        //     IRouter.CodeCommitment memory codeCommitment = codeCommitmentsArray[i];
        //     codesBytes =
        //         bytes.concat(codesBytes, keccak256(abi.encodePacked(codeCommitment.codeId, codeCommitment.approved)));
        // }

        // router.commitCodes(codeCommitmentsArray, createSignatures(codesBytes));
    }

    function commitBlocks(IRouter.BlockCommitment[] memory commitments) private {
        // bytes memory message;

        // for (uint256 i = 0; i < commitments.length; i++) {
        //     IRouter.BlockCommitment memory commitment = commitments[i];
        //     message = bytes.concat(message, commitBlock(commitment));
        // }

        // router.commitBlocks(commitments, createSignatures(message));
    }

    function commitBlock(IRouter.BlockCommitment memory commitment) private pure returns (bytes32) {
        // bytes memory commitmentsBytes;
        // for (uint256 i = 0; i < commitment.transitions.length; i++) {
        //     IRouter.StateTransition memory transition = commitment.transitions[i];

        //     bytes memory transitionsBytes;
        //     for (uint256 j = 0; j < transition.outgoingMessages.length; j++) {
        //         IRouter.OutgoingMessage memory outgoingMessage = transition.outgoingMessages[j];
        //         transitionsBytes = bytes.concat(
        //             transitionsBytes,
        //             keccak256(
        //                 abi.encodePacked(
        //                     outgoingMessage.messageId,
        //                     outgoingMessage.destination,
        //                     outgoingMessage.payload,
        //                     outgoingMessage.value,
        //                     outgoingMessage.replyDetails.replyTo,
        //                     outgoingMessage.replyDetails.replyCode
        //                 )
        //             )
        //         );
        //     }

        //     commitmentsBytes = bytes.concat(
        //         commitmentsBytes,
        //         keccak256(
        //             abi.encodePacked(
        //                 transition.actorId,
        //                 transition.oldStateHash,
        //                 transition.newStateHash,
        //                 keccak256(transitionsBytes)
        //             )
        //         )
        //     );
        // }

        // return keccak256(
        //     abi.encodePacked(
        //         commitment.blockHash,
        //         commitment.allowedPredBlockHash,
        //         commitment.allowedPrevCommitmentHash,
        //         keccak256(commitmentsBytes)
        //     )
        // );
    }

    function createSignatures(bytes memory message) private view returns (bytes[] memory) {
        // bytes[] memory signatures = new bytes[](validatorsPrivateKeys.length);
        // bytes32 messageHash = address(router).toDataWithIntendedValidatorHash(abi.encodePacked(keccak256(message)));

        // for (uint256 i = 0; i < validatorsPrivateKeys.length; i++) {
        //     uint256 validatorPrivateKey = validatorsPrivateKeys[i];
        //     (uint8 v, bytes32 r, bytes32 s) = vm.sign(validatorPrivateKey, messageHash);
        //     signatures[i] = abi.encodePacked(r, s, v);
        // }

        // return signatures;
    }
}
