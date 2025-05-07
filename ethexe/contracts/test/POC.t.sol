// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {IVaultConfigurator} from "symbiotic-core/src/interfaces/IVaultConfigurator.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {IBaseDelegator} from "symbiotic-core/src/interfaces/delegator/IBaseDelegator.sol";
import {IOperatorSpecificDelegator} from "symbiotic-core/src/interfaces/delegator/IOperatorSpecificDelegator.sol";
import {IVetoSlasher} from "symbiotic-core/src/interfaces/slasher/IVetoSlasher.sol";
import {IBaseSlasher} from "symbiotic-core/src/interfaces/slasher/IBaseSlasher.sol";
import {SigningKey, FROSTOffchain} from "frost-secp256k1-evm/FROSTOffchain.sol";
import {Vm} from "forge-std/Vm.sol";

import {Gear} from "../src/libraries/Gear.sol";
import {Base} from "./Base.t.sol";
import {IMirror} from "../src/Mirror.sol";
import {IRouter} from "../src/IRouter.sol";

contract POCTest is Base {
    using MessageHashUtils for address;
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using FROSTOffchain for SigningKey;

    SigningKey signingKey;
    EnumerableMap.AddressToUintMap private operators;
    address[] private vaults;

    function setUp() public override {
        admin = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;
        eraDuration = 400;
        electionDuration = 100;
        blockDuration = 12;
        maxValidators = 3;

        setUpWrappedVara();

        setUpMiddleware();

        signingKey = FROSTOffchain.newSigningKey();
        Vm.Wallet memory publicKey = vm.createWallet(signingKey.asScalar());

        for (uint256 i = 0; i < 10; i++) {
            (address _addr, uint256 _key) = makeAddrAndKey(vm.toString(i + 1));
            operators.set(_addr, _key);
            address _vault = createOperatorWithStake(_addr, (i + 1) * 1000);
            vaults.push(_vault);
        }

        vm.warp(vm.getBlockTimestamp() + 1);
        address[] memory _validators = middleware.makeElectionAt(uint48(vm.getBlockTimestamp()) - 1, maxValidators);

        setUpRouter(Gear.AggregatedPublicKey(publicKey.publicKeyX, publicKey.publicKeyY), _validators);

        // Change slash requester and executor to router
        // Note: just to check that it is possible to change them for now and do not affect the poc test
        vm.startPrank(admin);
        {
            middleware.changeSlashRequester(address(router));
            middleware.changeSlashExecutor(address(router));
        }
        vm.stopPrank();
    }

    function test_POC() public {
        bytes32 _codeId = bytes32(uint256(1));

        bytes32[] memory hashes = new bytes32[](1);
        hashes[0] = bytes32(uint256(1));
        vm.blobhashes(hashes);

        router.requestCodeValidation(_codeId);

        address[] memory _validators = router.validators();
        assertEq(_validators.length, maxValidators);

        uint256[] memory _privateKeys = new uint256[](1);
        _privateKeys[0] = signingKey.asScalar();

        commitCode(_privateKeys, Gear.CodeCommitment(_codeId, 42, true));

        address _ping = deployPing(_privateKeys, _codeId);
        IMirror actor = IMirror(_ping);
        assertEq(router.latestCommittedBlockHash(), blockHash(vm.getBlockNumber() - 1));
        assertEq(actor.stateHash(), bytes32(uint256(1)));
        assertEq(actor.nonce(), uint256(1));

        doPingPong(_privateKeys, _ping);
        assertEq(router.latestCommittedBlockHash(), blockHash(vm.getBlockNumber() - 1));
        assertEq(actor.stateHash(), bytes32(uint256(2)));
        assertEq(actor.nonce(), uint256(2));

        // Check that going to next era without re-election is ok and old validators are still valid.
        rollBlocks(eraDuration / blockDuration);
        doPingPong(_privateKeys, _ping);
        assertEq(router.latestCommittedBlockHash(), blockHash(vm.getBlockNumber() - 1));
        assertEq(actor.stateHash(), bytes32(uint256(2)));
        assertEq(actor.nonce(), uint256(3));

        // Change validators stake and make re-election
        depositInto(vaults[0], 10_000);
        depositInto(vaults[1], 10_000);
        depositInto(vaults[2], 10_000);
        rollBlocks((eraDuration - electionDuration) / blockDuration);

        SigningKey _signingKey = FROSTOffchain.newSigningKey();
        Vm.Wallet memory _publicKey = vm.createWallet(_signingKey.asScalar());

        // TODO: makeElectionAt should also return Gear.AggregatedPublicKey
        _validators = middleware.makeElectionAt(uint48(vm.getBlockTimestamp()) - 1, maxValidators);

        commitValidators(
            _privateKeys,
            Gear.ValidatorsCommitment(
                Gear.AggregatedPublicKey(_publicKey.publicKeyX, _publicKey.publicKeyY), "", _validators, 2
            )
        );

        for (uint256 i = 0; i < _validators.length; i++) {
            address _operator = _validators[i];

            // Check that election is correct
            // Validators are sorted in descending order
            (address expected,) = makeAddrAndKey(vm.toString(_validators.length - i));
            assertEq(_operator, expected);
        }

        _privateKeys[0] = _signingKey.asScalar();

        // Go to a new era and commit from new validators
        rollBlocks(electionDuration / blockDuration);
        doPingPong(_privateKeys, _ping);
        assertEq(router.latestCommittedBlockHash(), blockHash(vm.getBlockNumber() - 1));
        assertEq(actor.stateHash(), bytes32(uint256(2)));
        assertEq(actor.nonce(), uint256(4));
    }

    function deployPing(uint256[] memory _privateKeys, bytes32 _codeId) private returns (address _ping) {
        vm.startPrank(admin, admin);
        {
            vm.expectEmit(true, false, false, false);
            emit IRouter.ProgramCreated(address(0), bytes32(uint256(1)));
            _ping = router.createProgram(_codeId, "salt", address(0));
            IMirror(_ping).sendMessage("PING", 0, false);
        }
        vm.stopPrank();

        uint48 _deploymentTimestamp = uint48(vm.getBlockTimestamp());
        bytes32 _deploymentBlock = blockHash(vm.getBlockNumber());

        rollBlocks(1);

        Gear.Message[] memory _outgoingMessages = new Gear.Message[](1);
        _outgoingMessages[0] = Gear.Message(
            0, // message id
            admin, // destination
            "PONG", // payload
            0, // value
            Gear.ReplyDetails(
                0, // reply to
                0 // reply code
            ),
            false // call
        );

        Gear.StateTransition[] memory _transitions = new Gear.StateTransition[](1);
        _transitions[0] = Gear.StateTransition(
            _ping, // actor id
            bytes32(uint256(1)), // new state hash
            false, // exited
            address(0), // inheritor
            uint128(0), // value to receive
            new Gear.ValueClaim[](0), // value claims
            _outgoingMessages // messages
        );

        vm.expectEmit(true, false, false, false);
        emit IMirror.Message(0, admin, "PONG", 0);
        commitBlock(
            _privateKeys,
            Gear.BlockCommitment(
                _deploymentBlock, // commitment block hash
                _deploymentTimestamp, // commitment block timestamp
                router.latestCommittedBlockHash(), // previously committed block hash
                _deploymentBlock, // predecessor block hash
                _transitions // commitment transitions
            )
        );
    }

    function doPingPong(uint256[] memory _privateKeys, address _ping) internal {
        vm.startPrank(admin, admin);
        {
            uint256 _allowanceBefore = wrappedVara.allowance(admin, _ping);
            wrappedVara.approve(_ping, type(uint256).max);
            IMirror(_ping).sendMessage("PING", 0, false);
            wrappedVara.approve(_ping, _allowanceBefore);
        }
        vm.stopPrank();

        uint48 _pingTimestamp = uint48(vm.getBlockTimestamp());
        bytes32 _pingBlock = blockHash(vm.getBlockNumber());

        rollBlocks(1);

        Gear.Message[] memory _outgoingMessages = new Gear.Message[](1);
        _outgoingMessages[0] = Gear.Message(
            0, // message id
            admin, // destination
            "PONG", // payload
            0, // value
            Gear.ReplyDetails(
                0, // reply to
                0 // reply code
            ),
            false // call
        );

        Gear.StateTransition[] memory _transitions = new Gear.StateTransition[](1);
        _transitions[0] = Gear.StateTransition(
            _ping, // actor id
            bytes32(uint256(2)), // new state hash
            false, // exited
            address(0), // inheritor
            0, // value to receive
            new Gear.ValueClaim[](0), // value claims
            _outgoingMessages // messages
        );

        vm.expectEmit(true, false, false, false);
        emit IMirror.Message(0, admin, "PONG", 0);
        commitBlock(
            _privateKeys,
            Gear.BlockCommitment(
                _pingBlock, // commitment block hash
                _pingTimestamp, // commitment block timestamp
                router.latestCommittedBlockHash(), // previously committed block hash
                _pingBlock, // predecessor block hash
                _transitions // commitment transitions
            )
        );
    }
}
