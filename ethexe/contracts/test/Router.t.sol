// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {console} from "forge-std/Test.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {Base} from "./Base.t.sol";

contract RouterTest is Base {
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

    function setUp() public override {
        admin = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;
        eraDuration = 1000;
        electionDuration = 100;
        blockDuration = 12;
        maxValidators = 3;
        validationDelay = 60;

        setUpWrappedVara();

        validators.push(alicePublic);
        validators.push(bobPublic);
        validators.push(charliePublic);

        validatorsPrivateKeys.push(alicePrivate);
        validatorsPrivateKeys.push(bobPrivate);
        validatorsPrivateKeys.push(charliePrivate);

        setUpRouter(validators);
    }

    function test_validatorsCommitment() public {
        address[] memory _validators = new address[](3);
        uint256[] memory _validatorPrivateKeys = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            (address addr, uint256 key) = makeAddrAndKey(vm.toString(i));
            _validators[i] = addr;
            _validatorPrivateKeys[i] = key;
        }

        Gear.ValidatorsCommitment memory commitment = Gear.ValidatorsCommitment(_validators, 1);

        // Election is not yet started
        vm.expectRevert();
        commitValidators(commitment);

        // Still not started
        vm.warp(router.genesisTimestamp() + eraDuration - electionDuration - 1);
        vm.expectRevert();
        commitValidators(commitment);

        vm.warp(router.genesisTimestamp() + eraDuration - electionDuration);

        // Started but wrong era index
        Gear.ValidatorsCommitment memory commitment2 = Gear.ValidatorsCommitment(_validators, 2);
        vm.expectRevert();
        commitValidators(commitment2);

        // Correct commitment
        commitValidators(commitment);

        // Try to set validators twice
        vm.expectRevert();
        commitValidators(commitment);

        // Validators are not updated yet
        assertEq(router.validators(), validators);

        vm.warp(router.genesisTimestamp() + eraDuration);

        // Validators are updated
        assertEq(router.validators(), _validators);

        // Update them locally
        validators = _validators;
        validatorsPrivateKeys = _validatorPrivateKeys;

        // Commit the same validators again
        vm.warp(vm.getBlockTimestamp() + eraDuration - electionDuration);
        commitValidators(commitment2);

        vm.warp(vm.getBlockTimestamp() + electionDuration);
        assertEq(router.validators(), _validators);

        // Do not commit validators - should continue to work with validators from previous era then.
        vm.warp(vm.getBlockTimestamp() + 10 * eraDuration);
        assertEq(router.validators(), _validators);

        // Try to commit from past validators
        uint256 currentEraIndex = (vm.getBlockTimestamp() - router.genesisTimestamp()) / eraDuration;
        vm.warp(router.genesisTimestamp() + (currentEraIndex + 1) * eraDuration - electionDuration);
        assertEq(router.validators(), _validators);
        commitment.eraIndex = currentEraIndex + 1;

        uint256[] memory wrongValidatorPrivateKeys = new uint256[](3);
        wrongValidatorPrivateKeys[0] = 1;
        wrongValidatorPrivateKeys[1] = 2;
        wrongValidatorPrivateKeys[2] = 3;
        assertNotEq(wrongValidatorPrivateKeys, validatorsPrivateKeys);

        vm.expectRevert();
        commitValidators(wrongValidatorPrivateKeys, commitment);
    }

    function test_lateCommitments() public {
        address[] memory _validators = new address[](3);
        uint256[] memory _validatorPrivateKeys = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            (address addr, uint256 key) = makeAddrAndKey(vm.toString(i));
            _validators[i] = addr;
            _validatorPrivateKeys[i] = key;
        }

        Gear.ValidatorsCommitment memory _commitment = Gear.ValidatorsCommitment(_validators, 1);

        vm.warp(router.genesisTimestamp() + eraDuration - electionDuration);
        commitValidators(_commitment);

        // Go to the next era, setting block hash to the last 1 blocks of the era
        vm.warp(router.genesisTimestamp() + eraDuration - uint48(blockDuration));
        rollBlocks(1);

        uint256 _eraStartNumber = vm.getBlockNumber();
        uint48 _eraStartTimestamp = uint48(vm.getBlockTimestamp());

        Gear.BlockCommitment memory _blockCommitment = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber - 1),
            timestamp: _eraStartTimestamp - uint48(blockDuration),
            previousCommittedBlock: router.latestCommittedBlockHash(),
            predecessorBlock: blockHash(_eraStartNumber - 1),
            transitions: new Gear.StateTransition[](0)
        });

        // Try to commit block from the previous era using new validators
        vm.expectRevert();
        commitBlock(_validatorPrivateKeys, _blockCommitment);

        // Now try to commit block from the previous era using old validators
        commitBlock(validatorsPrivateKeys, _blockCommitment);

        rollBlocks(1);
        _blockCommitment = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber),
            timestamp: _eraStartTimestamp,
            previousCommittedBlock: router.latestCommittedBlockHash(),
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });

        // Try to commit block from the new era using old validators
        vm.expectRevert();
        commitBlock(validatorsPrivateKeys, _blockCommitment);

        // Now try to commit block from the new era using new validators
        commitBlock(_validatorPrivateKeys, _blockCommitment);
    }

    function test_lateCommitmentsAfterDelay() public {
        address[] memory _validators = new address[](3);
        uint256[] memory _validatorPrivateKeys = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            (address addr, uint256 key) = makeAddrAndKey(vm.toString(i));
            _validators[i] = addr;
            _validatorPrivateKeys[i] = key;
        }

        Gear.ValidatorsCommitment memory _commitment = Gear.ValidatorsCommitment(_validators, 1);

        vm.warp(router.genesisTimestamp() + eraDuration - electionDuration);
        commitValidators(_commitment);

        // Go to the next era, setting block hash to the last 5 blocks of the era and first 5 blocks of the new era
        vm.warp(router.genesisTimestamp() + eraDuration - 5 * uint48(blockDuration));
        rollBlocks(10);

        Gear.BlockCommitment memory _blockCommitment = Gear.BlockCommitment({
            hash: blockHash(vm.getBlockNumber() - 6),
            timestamp: uint48(vm.getBlockTimestamp() - 6 * blockDuration),
            previousCommittedBlock: router.latestCommittedBlockHash(),
            predecessorBlock: blockHash(vm.getBlockNumber() - 1),
            transitions: new Gear.StateTransition[](0)
        });

        // Try to commit block from the previous era using old validators
        // Must be failed because the validation delay is already passed
        vm.expectRevert();
        commitBlock(validatorsPrivateKeys, _blockCommitment);

        // Now try to commit block from the previous era using new validators
        // Must be successful because the validation delay is already passed
        commitBlock(_validatorPrivateKeys, _blockCommitment);
    }

    function test_manyLateCommitments() public {
        address[] memory _validators = new address[](3);
        uint256[] memory _validatorPrivateKeys = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            (address addr, uint256 key) = makeAddrAndKey(vm.toString(i));
            _validators[i] = addr;
            _validatorPrivateKeys[i] = key;
        }

        Gear.ValidatorsCommitment memory _commitment = Gear.ValidatorsCommitment(_validators, 1);

        vm.warp(router.genesisTimestamp() + eraDuration - electionDuration);
        commitValidators(_commitment);

        // Go to the next era, setting block hash to the last 4 blocks of the era
        vm.warp(router.genesisTimestamp() + eraDuration - 4 * uint48(blockDuration));
        rollBlocks(4);

        uint256 _eraStartNumber = vm.getBlockNumber();
        uint48 _eraStartTimestamp = uint48(vm.getBlockTimestamp());

        // Try to commit blocks: [n - 4] <- [n - 3] <- [n]
        // Where [n] is a start of the new era
        Gear.BlockCommitment[] memory _commitments = new Gear.BlockCommitment[](3);
        _commitments[0] = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber - 4),
            timestamp: _eraStartTimestamp - 4 * uint48(blockDuration),
            previousCommittedBlock: router.latestCommittedBlockHash(),
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });
        _commitments[1] = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber - 3),
            timestamp: _eraStartTimestamp - 3 * uint48(blockDuration),
            previousCommittedBlock: _commitments[0].hash,
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });
        _commitments[2] = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber),
            timestamp: _eraStartTimestamp,
            previousCommittedBlock: _commitments[1].hash,
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });

        // Roll to next block to be possible to make commitment for the era start block
        rollBlocks(1);

        // Validation must fail because the last block is from new era, so must be committed by new validators
        vm.expectRevert();
        commitBlocks(validatorsPrivateKeys, _commitments);

        // Now try to commit [n - 4] <- [n - 3] <- [n - 2]
        _commitments[2] = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber - 2),
            timestamp: _eraStartTimestamp - 2 * uint48(blockDuration),
            previousCommittedBlock: _commitments[1].hash,
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });
        // Must be successful, because all blocks are from the previous era
        commitBlocks(validatorsPrivateKeys, _commitments);

        // Now try to commit [n - 1] <- [n] <- [n + 1] using new validators
        rollBlocks(1);
        _commitments[0] = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber - 1),
            timestamp: _eraStartTimestamp - uint48(blockDuration),
            previousCommittedBlock: router.latestCommittedBlockHash(),
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });
        _commitments[1] = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber),
            timestamp: _eraStartTimestamp,
            previousCommittedBlock: _commitments[0].hash,
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });
        _commitments[2] = Gear.BlockCommitment({
            hash: blockHash(_eraStartNumber + 1),
            timestamp: _eraStartTimestamp + uint48(blockDuration),
            previousCommittedBlock: _commitments[1].hash,
            predecessorBlock: blockHash(_eraStartNumber),
            transitions: new Gear.StateTransition[](0)
        });
        // Must be successful, because the newest blocks are from the new era
        commitBlocks(_validatorPrivateKeys, _commitments);
    }

    /* helper functions */

    function commitValidators(Gear.ValidatorsCommitment memory commitment) private {
        commitValidators(validatorsPrivateKeys, commitment);
    }
}
