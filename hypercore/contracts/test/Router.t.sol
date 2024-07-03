// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Test, console} from "forge-std/Test.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {IProgram, Program} from "../src/Program.sol";
import {IRouter, Router} from "../src/Router.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

contract RouterTest is Test {
    using MessageHashUtils for address;

    address public deployerAddress;

    address[] public validatorsArray;
    uint256[] public validatorsPrivateKeys;

    WrappedVara public wrapperVara;
    Program public program;
    Router public router;

    function setUp() public {
        deployerAddress = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;

        validatorsArray.push(0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1);
        validatorsPrivateKeys.push(0x32816f851b9cc71c4eb956214ded8cf481f7af66c125d1fb9deae366ae4f13a6);

        startPrank(deployerAddress);

        wrapperVara = new WrappedVara(deployerAddress, 6);

        address programAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 1);
        address wrappedVaraAddress = address(wrapperVara);

        router = new Router(deployerAddress, programAddress, wrappedVaraAddress, validatorsArray);
        program = new Program(address(router));

        wrapperVara.approve(address(router), type(uint256).max);

        assertEq(router.program(), address(program));
        assertEq(program.router(), address(router));
    }

    function test_DemoPing() public {
        startPrank(deployerAddress);

        bytes32 codeId = bytes32(uint256(1));
        bytes32 blobTx = bytes32(uint256(2));

        router.uploadCode(codeId, blobTx);

        IRouter.CodeCommitment[] memory codeCommitmentsArray = new IRouter.CodeCommitment[](1);
        codeCommitmentsArray[0] = IRouter.CodeCommitment(codeId, true);

        commitCodes(codeCommitmentsArray);

        address actorId = router.createProgram(codeId, "salt", "PING", 1_000_000_000);
        IProgram deployedProgram = IProgram(actorId);

        assertEq(deployedProgram.router(), address(router));
        assertEq(deployedProgram.stateHash(), 0);

        IRouter.OutgoingMessage[] memory outgoingMessages = new IRouter.OutgoingMessage[](1);
        outgoingMessages[0] = IRouter.OutgoingMessage(deployerAddress, "PONG", 0, IRouter.ReplyDetails(0, 0));

        IRouter.TransitionCommitment[] memory transitionsCommitmentsArray = new IRouter.TransitionCommitment[](1);
        transitionsCommitmentsArray[0] = IRouter.TransitionCommitment(actorId, 0, bytes32(uint256(1)), outgoingMessages);

        commitTransitions(transitionsCommitmentsArray);

        assertEq(deployedProgram.stateHash(), bytes32(uint256(1)));
    }

    function startPrank(address msgSender) private {
        vm.startPrank(msgSender, msgSender);
    }

    function commitCodes(IRouter.CodeCommitment[] memory codeCommitmentsArray) private {
        bytes memory message;

        for (uint256 i = 0; i < codeCommitmentsArray.length; i++) {
            IRouter.CodeCommitment memory codeCommitment = codeCommitmentsArray[i];
            message = bytes.concat(message, keccak256(abi.encodePacked(codeCommitment.codeId, codeCommitment.approved)));
        }

        router.commitCodes(codeCommitmentsArray, createSignatures(message));
    }

    function commitTransitions(IRouter.TransitionCommitment[] memory transitionsCommitmentsArray) private {
        bytes memory message;

        for (uint256 i = 0; i < transitionsCommitmentsArray.length; i++) {
            IRouter.TransitionCommitment memory transitionCommitment = transitionsCommitmentsArray[i];
            bytes memory message1;

            for (uint256 j = 0; j < transitionCommitment.outgoingMessages.length; j++) {
                IRouter.OutgoingMessage memory outgoingMessage = transitionCommitment.outgoingMessages[j];
                message1 = bytes.concat(
                    message1,
                    keccak256(
                        abi.encodePacked(
                            outgoingMessage.destination,
                            outgoingMessage.payload,
                            outgoingMessage.value,
                            outgoingMessage.replyDetails.replyTo,
                            outgoingMessage.replyDetails.replyCode
                        )
                    )
                );
            }

            message = bytes.concat(
                message,
                keccak256(
                    abi.encodePacked(
                        transitionCommitment.actorId,
                        transitionCommitment.oldStateHash,
                        transitionCommitment.newStateHash,
                        keccak256(message1)
                    )
                )
            );
        }

        router.commitTransitions(transitionsCommitmentsArray, createSignatures(message));
    }

    function createSignatures(bytes memory message) private view returns (bytes[] memory) {
        bytes[] memory signatures = new bytes[](validatorsPrivateKeys.length);
        bytes32 messageHash = address(router).toDataWithIntendedValidatorHash(abi.encodePacked(keccak256(message)));

        for (uint256 i = 0; i < validatorsPrivateKeys.length; i++) {
            uint256 validatorPrivateKey = validatorsPrivateKeys[i];
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(validatorPrivateKey, messageHash);
            signatures[i] = abi.encodePacked(r, s, v);
        }

        return signatures;
    }
}
