// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.35;

import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Vm} from "forge-std/Vm.sol";
import {FROSTOffchain, SigningKey} from "frost-secp256k1-evm/FROSTOffchain.sol";
import {IRouter} from "src/IRouter.sol";
import {IMirror} from "src/Mirror.sol";
import {Gear} from "src/libraries/Gear.sol";
import {Base} from "test/Base.t.sol";

contract POCTest is Base {
    using MessageHashUtils for address;
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using FROSTOffchain for SigningKey;
    using Gear for Gear.ValueClaim;

    bytes32 private constant EIP712_DOMAIN_TYPEHASH =
        keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
    bytes32 private constant PERMIT_TYPEHASH =
        keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)");
    bytes32 private constant REQUEST_CODE_VALIDATION_ON_BEHALF_TYPEHASH = keccak256(
        "RequestCodeValidationOnBehalf(address requester,bytes32 codeId,bytes32[] blobHashes,uint256 nonce,uint256 deadline)"
    );

    SigningKey signingKey;
    EnumerableMap.AddressToUintMap private operators;
    address[] private vaults;
    address private sender;
    uint256 private senderPrivateKey;
    address private relayer;
    uint256 private relayerPrivateKey;

    function setUp() public override {
        admin = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;
        eraDuration = 400;
        electionDuration = 100;
        blockDuration = 12;
        maxValidators = 3;
        (sender, senderPrivateKey) = makeAddrAndKey("Sender");
        (relayer, relayerPrivateKey) = makeAddrAndKey("Relayer");

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
        uint256 baseFee = router.requestCodeValidationBaseFee();

        vm.startPrank(admin);
        {
            bool success = wrappedVara.transfer(sender, baseFee);
            require(success, "Transfer failed");
        }
        vm.stopPrank();

        vm.startPrank(sender);

        uint256 deadline = vm.getBlockTimestamp() + 10;
        bytes32 structHash = keccak256(
            abi.encode(PERMIT_TYPEHASH, sender, address(router), baseFee, wrappedVara.nonces(sender), deadline)
        );
        bytes32 hash = MessageHashUtils.toTypedDataHash(wrappedVara.DOMAIN_SEPARATOR(), structHash);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(senderPrivateKey, hash);

        bytes32 _codeId = bytes32(uint256(1));

        bytes32[] memory hashes = new bytes32[](1);
        hashes[0] = bytes32(uint256(1));
        vm.blobhashes(hashes);

        router.requestCodeValidation(_codeId, deadline, v, r, s);

        address[] memory _validators = router.validators();
        assertEq(_validators.length, maxValidators);

        uint256[] memory _privateKeys = new uint256[](1);
        _privateKeys[0] = signingKey.asScalar();

        rollBlocks(1);
        commitCode(_privateKeys, Gear.CodeCommitment(_codeId, true));

        address _piggyBank = doPiggyBankValueClaim(_privateKeys, _codeId);
        IMirror actor = IMirror(_piggyBank);
        assertEq(actor.stateHash(), bytes32(uint256(2)));

        address _ping = deployPing(_privateKeys, _codeId);
        actor = IMirror(_ping);
        assertEq(actor.stateHash(), bytes32(uint256(1)));
        assertEq(actor.nonce(), uint256(1));

        doPingPong(_privateKeys, _ping);
        assertEq(actor.stateHash(), bytes32(uint256(2)));
        assertEq(actor.nonce(), uint256(2));

        // Check that going to next era without re-election is ok and old validators are still valid.
        rollBlocks(eraDuration / blockDuration);
        doPingPong(_privateKeys, _ping);
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
                true, Gear.AggregatedPublicKey(_publicKey.publicKeyX, _publicKey.publicKeyY), "", _validators, 2
            ),
            false
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
        assertEq(actor.stateHash(), bytes32(uint256(2)));
        assertEq(actor.nonce(), uint256(4));

        vm.stopPrank();
    }

    function test_requestCodeValidationOnBehalf() public {
        uint256 baseFee = router.requestCodeValidationBaseFee();
        uint256 extraFee = router.requestCodeValidationExtraFee();

        uint256 fee = baseFee + extraFee;

        vm.startPrank(admin);
        {
            bool success = wrappedVara.transfer(sender, fee);
            require(success, "Transfer failed");
        }
        vm.stopPrank();

        assertEq(wrappedVara.balanceOf(sender), fee);

        vm.startPrank(relayer);

        bytes32 _codeId = bytes32(uint256(1));
        bytes32[] memory blobHashes = new bytes32[](1);
        blobHashes[0] = bytes32(uint256(1));
        uint256 deadline = vm.getBlockTimestamp() + 10;

        bytes32 structHash1 = keccak256(
            abi.encode(
                REQUEST_CODE_VALIDATION_ON_BEHALF_TYPEHASH,
                sender,
                _codeId,
                keccak256(abi.encodePacked(blobHashes)),
                router.nonces(sender),
                deadline
            )
        );
        bytes32 hash1 = MessageHashUtils.toTypedDataHash(router.DOMAIN_SEPARATOR(), structHash1);
        (uint8 v1, bytes32 r1, bytes32 s1) = vm.sign(senderPrivateKey, hash1);

        bytes32 structHash2 =
            keccak256(abi.encode(PERMIT_TYPEHASH, sender, address(router), fee, wrappedVara.nonces(sender), deadline));
        bytes32 hash2 = MessageHashUtils.toTypedDataHash(wrappedVara.DOMAIN_SEPARATOR(), structHash2);
        (uint8 v2, bytes32 r2, bytes32 s2) = vm.sign(senderPrivateKey, hash2);

        vm.blobhashes(blobHashes);

        router.requestCodeValidationOnBehalf(sender, _codeId, blobHashes, deadline, v1, r1, s1, v2, r2, s2);

        assertEq(wrappedVara.balanceOf(sender), 0);

        vm.stopPrank();
    }

    function deployPing(uint256[] memory _privateKeys, bytes32 _codeId) private returns (address _ping) {
        vm.startPrank(admin, admin);
        {
            vm.expectEmit(true, false, false, false);
            emit IRouter.ProgramCreated(address(0), bytes32(uint256(1)));
            _ping = router.createProgram(_codeId, "salt", address(0));
            IMirror(_ping).sendMessage("PING", false);
        }
        vm.stopPrank();

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
            false, // value to receive negative sign
            bytes32(uint256(2)), // merkle root
            _outgoingMessages // messages
        );

        vm.expectEmit(true, false, false, false);
        emit IMirror.Message(0, admin, "PONG", 0);
        commitBlock(_privateKeys, _transitions);
    }

    function doPingPong(uint256[] memory _privateKeys, address _ping) internal {
        vm.startPrank(admin, admin);
        {
            IMirror(_ping).sendMessage("PING", false);
        }
        vm.stopPrank();

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
            false, // value to receive negative sign
            bytes32(uint256(3)), // merkle root
            _outgoingMessages // messages
        );

        vm.expectEmit(true, false, false, false);
        emit IMirror.Message(0, admin, "PONG", 0);
        commitBlock(_privateKeys, _transitions);
    }

    function doPiggyBankValueClaim(uint256[] memory _privateKeys, bytes32 _codeId)
        private
        returns (address _piggyBank)
    {
        vm.startPrank(admin, admin);
        {
            vm.expectEmit(true, false, false, false);
            emit IRouter.ProgramCreated(address(0), bytes32(uint256(1)));
            _piggyBank = router.createProgram(_codeId, "salt1", address(0));
            vm.deal(address(router), 1 ether);
        }
        vm.stopPrank();

        address depositReceiver = makeAddr("depositReceiver");
        assertEq(depositReceiver.balance, 0 ether);

        Gear.ValueClaim memory valueClaim =
            Gear.ValueClaim({messageId: bytes32(uint256(2)), destination: depositReceiver, value: 1 ether});

        // [H256::from([0x11; 32]), H256::from([0x22; 32]), valueClaim.outgoingActionHash()]
        Gear.StateTransition[] memory _transitions = new Gear.StateTransition[](1);
        _transitions[0] = Gear.StateTransition(
            _piggyBank, // actor id
            bytes32(uint256(2)), // new state hash
            false, // exited
            address(0), // inheritor
            uint128(1 ether), // value to receive
            false, // value to receive negative sign
            0x929735af7aa0c21d834f89c8f8e54d2a0342eb202f7940192f9c8b49fe6de4eb, // merkle root
            new Gear.Message[](0) // messages
        );

        commitBlock(_privateKeys, _transitions);

        bytes32[] memory proof = new bytes32[](1);
        proof[0] = 0x3e92e0db88d6afea9edc4eedf62fffa4d92bcdfc310dccbe943747fe8302e871;
        IMirror(_piggyBank).processOutgoingAction(_transitions[0].newStateHash, 3, 2, valueClaim.pack(), proof);
        assertEq(depositReceiver.balance, 1 ether);
    }
}
