// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Vm, console} from "forge-std/Test.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {SigningKey, FROSTOffchain} from "frost-secp256k1-evm/FROSTOffchain.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {Base} from "./Base.t.sol";

contract RouterTest is Base {
    using MessageHashUtils for address;
    using FROSTOffchain for SigningKey;

    address immutable deployer = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;

    address immutable alicePublic = 0x45D6536E3D4AdC8f4e13c5c4aA54bE968C55Abf1;
    uint256 immutable alicePrivate = 0x32816f851b9cc71c4eb956214ded8cf481f7af66c125d1fb9deae366ae4f13a6;

    address immutable bobPublic = 0xeFDa593324697918773069E0226dAD49d702f4D8;
    uint256 immutable bobPrivate = 0x068cc4910d9f5aae82f66d926757fde55a7d2e72b25b2a243606e5147712a450;

    address immutable charliePublic = 0x84de3f115eC548A32CcC9464D14376f888ab49e1;
    uint256 immutable charliePrivate = 0xa3f79c90a74fd984fd9c2a9c4286c53ad5ac38e32123e06720e9211566378bc4;

    SigningKey public signingKey;
    address[] public validators;
    uint256[] public validatorsPrivateKeys;

    function setUp() public override {
        admin = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;
        eraDuration = 1200;
        electionDuration = 120;
        blockDuration = 12;
        maxValidators = 3;
        validationDelay = 60;

        setUpWrappedVara();

        signingKey = FROSTOffchain.newSigningKey();
        Vm.Wallet memory publicKey = vm.createWallet(signingKey.asScalar());

        validators.push(alicePublic);
        validators.push(bobPublic);
        validators.push(charliePublic);

        validatorsPrivateKeys.push(alicePrivate);
        validatorsPrivateKeys.push(bobPrivate);
        validatorsPrivateKeys.push(charliePrivate);

        setUpRouter(Gear.AggregatedPublicKey(publicKey.publicKeyX, publicKey.publicKeyY), validators);
    }

    function test_validatorsCommitment() public {
        address[] memory _validators = new address[](3);
        uint256[] memory _validatorPrivateKeys = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            (address addr, uint256 key) = makeAddrAndKey(vm.toString(i));
            _validators[i] = addr;
            _validatorPrivateKeys[i] = key;
        }

        SigningKey _signingKey = FROSTOffchain.newSigningKey();
        Vm.Wallet memory _publicKey = vm.createWallet(_signingKey.asScalar());

        Gear.ValidatorsCommitment memory commitment = Gear.ValidatorsCommitment(
            Gear.AggregatedPublicKey(_publicKey.publicKeyX, _publicKey.publicKeyY), "", _validators, 1
        );

        // Revert - election is not yet started
        commitValidators(commitment, true);

        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + eraDuration - electionDuration) - 2 * blockDuration);
        // Revert - still not started (one block before election, because commitment goes to the next block)
        commitValidators(commitment, true);

        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + eraDuration - electionDuration));

        Gear.ValidatorsCommitment memory commitment2 = Gear.ValidatorsCommitment(
            Gear.AggregatedPublicKey(_publicKey.publicKeyX, _publicKey.publicKeyY), "", _validators, 2
        );
        // Revert - election started, but wrong era index in commitment
        commitValidators(commitment2, true);

        // Correct commitment
        commitValidators(commitment, false);

        // Revert - try to set validators twice in the same era
        commitValidators(commitment, true);

        // Validators are not updated yet
        assertEq(router.validators(), validators);

        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + eraDuration));

        // Validators are updated
        assertEq(router.validators(), _validators);

        // Update them locally
        signingKey = _signingKey;
        validators = _validators;
        validatorsPrivateKeys = _validatorPrivateKeys;

        // Commit the same validators again
        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + 2 * eraDuration - electionDuration));
        commitValidators(commitment2, false);

        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + 2 * eraDuration));
        assertEq(router.validators(), _validators);

        // Do not commit validators - should continue to work with validators from previous era then.
        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + 10 * eraDuration));
        assertEq(router.validators(), _validators);

        // Try to commit from wrong validators
        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + 11 * eraDuration - electionDuration));
        commitment.eraIndex = 10;

        uint256[] memory wrongValidatorPrivateKeys = new uint256[](3);
        wrongValidatorPrivateKeys[0] = 1;
        wrongValidatorPrivateKeys[1] = 2;
        wrongValidatorPrivateKeys[2] = 3;
        assertNotEq(wrongValidatorPrivateKeys, validatorsPrivateKeys);

        commitValidators(wrongValidatorPrivateKeys, commitment, true);
    }

    function test_emptyValidatorsCommitment() public {
        address[] memory _validators = new address[](0);

        SigningKey _signingKey = FROSTOffchain.newSigningKey();
        Vm.Wallet memory _publicKey = vm.createWallet(_signingKey.asScalar());

        Gear.ValidatorsCommitment memory commitment = Gear.ValidatorsCommitment(
            Gear.AggregatedPublicKey(_publicKey.publicKeyX, _publicKey.publicKeyY), "", _validators, 1
        );

        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + eraDuration - electionDuration) - 2 * blockDuration);
        rollOneBlockAndWarp(uint256(router.genesisTimestamp() + eraDuration - electionDuration));

        // Revert - empty validators list
        commitValidators(commitment, true);
    }

    function test_lateCommitments() public {
        address[] memory _validators = new address[](3);
        uint256[] memory _validatorPrivateKeys = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            (address addr, uint256 key) = makeAddrAndKey(vm.toString(i));
            _validators[i] = addr;
            _validatorPrivateKeys[i] = key;
        }

        SigningKey _signingKey = FROSTOffchain.newSigningKey();
        Vm.Wallet memory _publicKey = vm.createWallet(_signingKey.asScalar());
        uint256[] memory _privateKeys = new uint256[](1);

        Gear.ValidatorsCommitment memory _commitment = Gear.ValidatorsCommitment(
            Gear.AggregatedPublicKey(_publicKey.publicKeyX, _publicKey.publicKeyY), "", _validators, 1
        );
        Gear.StateTransition[] memory _transitions = new Gear.StateTransition[](0);

        rollOneBlockAndWarp(router.genesisTimestamp() + eraDuration - electionDuration);
        commitValidators(_commitment, false);

        // Goes to the next era
        rollOneBlockAndWarp(router.genesisTimestamp() + eraDuration);
        uint256 _eraStartNumber = vm.getBlockNumber();
        uint48 _eraStartTimestamp = uint48(vm.getBlockTimestamp());

        // Revert - try to commit batch from the previous era using new validators
        _privateKeys[0] = _signingKey.asScalar();
        commitBlock(_privateKeys, _transitions, blockHash(_eraStartNumber - 1), _eraStartTimestamp - 1, true);

        // Now try to commit block from the previous era using old validators
        _privateKeys[0] = signingKey.asScalar();
        commitBlock(_privateKeys, _transitions, blockHash(_eraStartNumber - 1), _eraStartTimestamp - 1, false);

        rollBlocks(1);

        // Revert - try to commit block from the new era using old validators
        _privateKeys[0] = signingKey.asScalar();
        commitBlock(_privateKeys, _transitions, blockHash(_eraStartNumber), _eraStartTimestamp, true);

        // Now try to commit block from the new era using new validators
        _privateKeys[0] = _signingKey.asScalar();
        commitBlock(_privateKeys, _transitions, blockHash(_eraStartNumber), _eraStartTimestamp, false);
    }

    function test_lateCommitmentsAfterDelay() public {
        address[] memory _validators = new address[](3);
        uint256[] memory _validatorPrivateKeys = new uint256[](3);
        for (uint256 i = 0; i < 3; i++) {
            (address addr, uint256 key) = makeAddrAndKey(vm.toString(i));
            _validators[i] = addr;
            _validatorPrivateKeys[i] = key;
        }

        SigningKey _signingKey = FROSTOffchain.newSigningKey();
        Vm.Wallet memory _publicKey = vm.createWallet(_signingKey.asScalar());
        uint256[] memory _privateKeys = new uint256[](1);

        Gear.ValidatorsCommitment memory _commitment = Gear.ValidatorsCommitment(
            Gear.AggregatedPublicKey(_publicKey.publicKeyX, _publicKey.publicKeyY), "", _validators, 1
        );
        Gear.StateTransition[] memory _transitions = new Gear.StateTransition[](0);

        rollOneBlockAndWarp(router.genesisTimestamp() + eraDuration - electionDuration);
        commitValidators(_commitment, false);

        // Goes to the next era
        rollOneBlockAndWarp(router.genesisTimestamp() + eraDuration);
        uint256 _eraStartNumber = vm.getBlockNumber();
        uint48 _eraStartTimestamp = uint48(vm.getBlockTimestamp());

        // Go after validation delay
        rollOneBlockAndWarp(_eraStartTimestamp + validationDelay);

        // Revert - try to commit block from the previous era using old validators after validation delay
        _privateKeys[0] = signingKey.asScalar();
        commitBlock(_privateKeys, _transitions, blockHash(_eraStartNumber - 1), _eraStartTimestamp - 1, true);

        // Now try to commit block from the previous era using new validators after validation delay
        _privateKeys[0] = _signingKey.asScalar();
        commitBlock(_privateKeys, _transitions, blockHash(_eraStartNumber - 1), _eraStartTimestamp - 1, false);
    }

    /* helper functions */

    function commitValidators(Gear.ValidatorsCommitment memory commitment, bool revertExpected) private {
        uint256[] memory _privateKeys = new uint256[](1);
        _privateKeys[0] = signingKey.asScalar();
        commitValidators(_privateKeys, commitment, revertExpected);
    }
}
