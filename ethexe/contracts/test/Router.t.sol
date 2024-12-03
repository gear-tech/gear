// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Test, console} from "forge-std/Test.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {IMirror, Mirror} from "../src/Mirror.sol";
import {MirrorProxy} from "../src/MirrorProxy.sol";
import {IRouter, Router} from "../src/Router.sol";
import {WrappedVara} from "../src/WrappedVara.sol";
import {Gear} from "../src/libraries/Gear.sol";

contract RouterTest is Test {
    using MessageHashUtils for address;

    address immutable deployer = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;

    address immutable alicePublic = 0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1;
    uint256 immutable alicePrivate = 0x32816f851b9cc71c4eb956214ded8cf481f7af66c125d1fb9deae366ae4f13a6;

    address immutable bobPublic = 0xeFDa593324697918773069E0226dAD49d702f4D8;
    uint256 immutable bobPrivate = 0x068cc4910d9f5aae82f66d926757fde55a7d2e72b25b2a243606e5147712a450;

    address immutable charliePublic = 0x84de3f115eC548A32CcC9464D14376f888ab49e1;
    uint256 immutable charliePrivate = 0xa3f79c90a74fd984fd9c2a9c4286c53ad5ac38e32123e06720e9211566378bc4;

    address[] public validators;
    uint256[] public validatorsPrivateKeys;

    Router public router;

    function setUp() public {
        validators.push(alicePublic);
        validators.push(bobPublic);
        validators.push(charliePublic);

        validatorsPrivateKeys.push(alicePrivate);
        validatorsPrivateKeys.push(bobPrivate);
        validatorsPrivateKeys.push(charliePrivate);

        startPrank(deployer);

        WrappedVara wrappedVara = WrappedVara(
            Upgrades.deployTransparentProxy(
                "WrappedVara.sol", deployer, abi.encodeCall(WrappedVara.initialize, (deployer))
            )
        );

        address mirrorAddress = vm.computeCreateAddress(deployer, vm.getNonce(deployer) + 2);
        address mirrorProxyAddress = vm.computeCreateAddress(deployer, vm.getNonce(deployer) + 3);

        router = Router(
            Upgrades.deployTransparentProxy(
                "Router.sol",
                deployer,
                abi.encodeCall(
                    Router.initialize, (deployer, mirrorAddress, mirrorProxyAddress, address(wrappedVara), validators)
                )
            )
        );

        vm.roll(vm.getBlockNumber() + 1);

        router.lookupGenesisHash();

        wrappedVara.approve(address(router), type(uint256).max);

        Mirror mirror = new Mirror();

        MirrorProxy mirrorProxy = new MirrorProxy(address(router));
        assertEq(mirrorProxy.router(), address(router));

        assertEq(router.mirrorImpl(), address(mirror));
        assertEq(router.mirrorProxyImpl(), address(mirrorProxy));
        assertEq(router.validators(), validators);
        assertEq(router.signingThresholdPercentage(), 6666);

        assertTrue(router.areValidators(validators));
    }

    function test_ping() public {
        startPrank(deployer);

        bytes32 codeId = bytes32(uint256(1));
        bytes32 blobTxHash = bytes32(uint256(2));

        router.requestCodeValidation(codeId, blobTxHash);

        Gear.CodeCommitment[] memory codeCommitments = new Gear.CodeCommitment[](1);
        codeCommitments[0] = Gear.CodeCommitment(codeId, true);

        assertEq(router.validators().length, 3);
        assertEq(router.validatorsThreshold(), 2);

        router.commitCodes(codeCommitments, _signCodeCommitments(codeCommitments));

        address actorId = router.createProgram(codeId, "salt", "PING", 1_000_000_000);
        IMirror actor = IMirror(actorId);

        assertEq(actor.router(), address(router));
        assertEq(actor.stateHash(), 0);
        assertEq(actor.inheritor(), address(0));

        vm.roll(vm.getBlockNumber() + 1);

        Gear.Message[] memory messages = new Gear.Message[](1);
        messages[0] = Gear.Message(
            0, // message id
            deployer, // destination
            "PONG", // payload
            0, // value
            Gear.ReplyDetails(
                0, // reply to
                0 // reply code
            )
        );

        Gear.StateTransition[] memory transitions = new Gear.StateTransition[](1);
        transitions[0] = Gear.StateTransition(
            actorId, // actor id
            bytes32(uint256(42)), // new state hash
            address(0), // inheritor
            uint128(0), // value to receive
            new Gear.ValueClaim[](0), // value claims
            messages // messages
        );

        Gear.BlockCommitment[] memory blockCommitments = new Gear.BlockCommitment[](1);
        blockCommitments[0] = Gear.BlockCommitment(
            bytes32(uint256(1)), // block hash
            uint48(0), // block timestamp
            bytes32(uint256(0)), // previous committed block
            blockhash(block.number - 1), // predecessor block
            transitions // transitions
        );

        router.commitBlocks(blockCommitments, _signBlockCommitments(blockCommitments));

        assertEq(router.latestCommittedBlockHash(), bytes32(uint256(1)));

        assertEq(actor.stateHash(), bytes32(uint256(42)));
        assertEq(actor.nonce(), uint256(1));
    }

    /* helper functions */

    function startPrank(address msgSender) private {
        vm.startPrank(msgSender, msgSender);
    }

    function _signBlockCommitments(Gear.BlockCommitment[] memory blockCommitments)
        private
        view
        returns (bytes[] memory)
    {
        bytes memory blockCommitmentsBytes;

        for (uint256 i = 0; i < blockCommitments.length; i++) {
            Gear.BlockCommitment memory blockCommitment = blockCommitments[i];

            bytes memory transitionsHashesBytes;

            for (uint256 j = 0; j < blockCommitment.transitions.length; j++) {
                Gear.StateTransition memory transition = blockCommitment.transitions[j];

                bytes memory valueClaimsBytes;
                for (uint256 k = 0; k < transition.valueClaims.length; k++) {
                    Gear.ValueClaim memory claim = transition.valueClaims[k];
                    valueClaimsBytes = bytes.concat(valueClaimsBytes, Gear.valueClaimBytes(claim));
                }

                bytes memory messagesHashesBytes;
                for (uint256 k = 0; k < transition.messages.length; k++) {
                    Gear.Message memory message = transition.messages[k];
                    messagesHashesBytes = bytes.concat(messagesHashesBytes, Gear.messageHash(message));
                }

                transitionsHashesBytes = bytes.concat(
                    transitionsHashesBytes,
                    Gear.stateTransitionHash(
                        transition.actorId,
                        transition.newStateHash,
                        transition.inheritor,
                        transition.valueToReceive,
                        keccak256(valueClaimsBytes),
                        keccak256(messagesHashesBytes)
                    )
                );
            }

            blockCommitmentsBytes = bytes.concat(
                blockCommitmentsBytes,
                Gear.blockCommitmentHash(
                    blockCommitment.hash,
                    blockCommitment.timestamp,
                    blockCommitment.previousCommittedBlock,
                    blockCommitment.predecessorBlock,
                    keccak256(transitionsHashesBytes)
                )
            );
        }

        return _signBytes(blockCommitmentsBytes);
    }

    function _signCodeCommitments(Gear.CodeCommitment[] memory codeCommitments) private view returns (bytes[] memory) {
        bytes memory codeCommitmentsBytes;

        for (uint256 i = 0; i < codeCommitments.length; i++) {
            Gear.CodeCommitment memory codeCommitment = codeCommitments[i];

            codeCommitmentsBytes =
                bytes.concat(codeCommitmentsBytes, keccak256(abi.encodePacked(codeCommitment.id, codeCommitment.valid)));
        }

        return _signBytes(codeCommitmentsBytes);
    }

    function _signBytes(bytes memory message) private view returns (bytes[] memory) {
        bytes[] memory signatures = new bytes[](validatorsPrivateKeys.length);

        bytes32 msgHash = address(router).toDataWithIntendedValidatorHash(abi.encodePacked(keccak256(message)));

        for (uint256 i = 0; i < validatorsPrivateKeys.length; i++) {
            uint256 validatorPrivateKey = validatorsPrivateKeys[i];

            (uint8 v, bytes32 r, bytes32 s) = vm.sign(validatorPrivateKey, msgHash);

            signatures[i] = abi.encodePacked(r, s, v);
        }

        return signatures;
    }
}
