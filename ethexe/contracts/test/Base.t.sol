// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {NetworkRegistry} from "symbiotic-core/src/contracts/NetworkRegistry.sol";
import {POCBaseTest} from "symbiotic-core/test/POCBase.t.sol";
import {IVaultConfigurator} from "symbiotic-core/src/interfaces/IVaultConfigurator.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {IBaseDelegator} from "symbiotic-core/src/interfaces/delegator/IBaseDelegator.sol";
import {IOperatorSpecificDelegator} from "symbiotic-core/src/interfaces/delegator/IOperatorSpecificDelegator.sol";
import {IVetoSlasher} from "symbiotic-core/src/interfaces/slasher/IVetoSlasher.sol";
import {IBaseSlasher} from "symbiotic-core/src/interfaces/slasher/IBaseSlasher.sol";

import {WrappedVara} from "../src/WrappedVara.sol";
import {IMirror, Mirror} from "../src/Mirror.sol";
import {MirrorProxy} from "../src/MirrorProxy.sol";
import {IRouter, Router} from "../src/Router.sol";
import {Middleware} from "../src/Middleware.sol";
import {Gear} from "../src/libraries/Gear.sol";

contract Base is POCBaseTest {
    using MessageHashUtils for address;
    using EnumerableMap for EnumerableMap.AddressToUintMap;

    address public admin;
    uint48 public eraDuration;
    uint48 public electionDuration;
    uint256 public validationDelay;
    uint256 public blockDuration;
    uint256 public maxValidators;

    Middleware public middleware;
    WrappedVara public wrappedVara;
    Router public router;
    Mirror public mirror;
    MirrorProxy public mirrorProxy;

    function setUp() public virtual override {
        revert("Must not be called");
    }

    function setUpWrappedVara() internal {
        require(admin != address(0), "Base: admin must be initialized");

        vm.startPrank(admin);
        {
            wrappedVara = WrappedVara(
                Upgrades.deployTransparentProxy(
                    "WrappedVara.sol", address(admin), abi.encodeCall(WrappedVara.initialize, (admin))
                )
            );
        }
        vm.stopPrank();
    }

    function setUpMiddleware() internal {
        require(admin != address(0), "Base: admin must be initialized");
        require(address(wrappedVara) != address(0), "Base: wrappedVara should be initialized");
        require(eraDuration > 0, "Base: eraDuration should be greater than 0");

        // For correct symbiotic work with time arithmetics
        vm.warp(eraDuration * 100);

        // set up the symbiotic ecosystem
        SYMBIOTIC_CORE_PROJECT_ROOT = "lib/symbiotic-core/";
        super.setUp();

        Middleware.Config memory cfg = Middleware.Config({
            eraDuration: eraDuration,
            minVaultEpochDuration: eraDuration * 2,
            operatorGracePeriod: eraDuration * 2,
            vaultGracePeriod: eraDuration * 2,
            minVetoDuration: eraDuration / 3,
            minSlashExecutionDelay: eraDuration / 3,
            maxResolverSetEpochsDelay: type(uint256).max,
            vaultRegistry: address(vaultFactory),
            allowedVaultImplVersion: 1,
            vetoSlasherImplType: 1,
            operatorRegistry: address(operatorRegistry),
            networkRegistry: address(networkRegistry),
            networkOptIn: address(operatorNetworkOptInService),
            middlewareService: address(networkMiddlewareService),
            collateral: address(wrappedVara),
            roleSlashRequester: admin,
            roleSlashExecutor: admin,
            vetoResolver: admin
        });

        middleware = new Middleware(cfg);
    }

    function setUpRouter(address[] memory _validators) internal {
        require(admin != address(0), "Base: admin must be initialized");
        require(address(wrappedVara) != address(0), "Base: wrappedVara should be initialized");
        require(eraDuration > 0, "Base: eraDuration should be greater than 0");
        require(electionDuration > 0, "Base: electionDuration should be greater than 0");
        require(blockDuration > 0, "Base: blockDuration should be greater than 0");

        address wrappedVaraAddress = address(wrappedVara);

        address mirrorAddress = vm.computeCreateAddress(admin, vm.getNonce(admin) + 2);
        address mirrorProxyAddress = vm.computeCreateAddress(admin, vm.getNonce(admin) + 3);

        vm.startPrank(admin, admin);
        {
            router = Router(
                Upgrades.deployTransparentProxy(
                    "Router.sol",
                    admin,
                    abi.encodeCall(
                        Router.initialize,
                        (
                            admin,
                            mirrorAddress,
                            mirrorProxyAddress,
                            wrappedVaraAddress,
                            uint256(eraDuration),
                            uint256(electionDuration),
                            uint256(validationDelay),
                            Gear.AggregatedPublicKey(
                                0x0000000000000000000000000000000000000000000000000000000000000001,
                                0x4218F20AE6C646B363DB68605822FB14264CA8D2587FDD6FBC750D587E76A7EE
                            ),
                            _validators
                        )
                    )
                )
            );
        }
        vm.stopPrank();

        rollBlocks(1);
        router.lookupGenesisHash();

        vm.startPrank(admin, admin);
        {
            mirror = new Mirror();
            mirrorProxy = new MirrorProxy(address(router));
        }
        vm.stopPrank();

        assertEq(router.mirrorImpl(), address(mirror));
        assertEq(router.mirrorProxyImpl(), address(mirrorProxy));
        assertEq(router.validators(), _validators);
        assertEq(router.signingThresholdPercentage(), 6666);
        assertTrue(router.areValidators(_validators));

        vm.startPrank(admin);
        {
            wrappedVara.approve(address(router), type(uint256).max);
        }
        vm.stopPrank();
    }

    function createOperatorWithStake(address _operator, uint256 _stake) internal returns (address _vault) {
        createOperator(_operator);
        _vault = createVaultForOperator(_operator);
        depositInto(_vault, _stake);
    }

    function depositInto(address _vault, uint256 _amount) internal {
        vm.startPrank(admin);
        {
            wrappedVara.approve(_vault, _amount);
            IVault(_vault).deposit(admin, _amount);
        }
        vm.stopPrank();
    }

    function createVaultForOperator(address _operator) internal returns (address _vault) {
        _vault = newVault(eraDuration * 2, _operator);

        vm.startPrank(_operator);
        {
            middleware.registerVault(_vault);
            operatorVaultOptInService.optIn(_vault);
            IOperatorSpecificDelegator(IVault(_vault).delegator()).setNetworkLimit(
                middleware.subnetwork(), type(uint256).max
            );
        }
        vm.stopPrank();
    }

    function commitValidators(uint256[] memory _privateKeys, Gear.ValidatorsCommitment memory commitment) internal {
        bytes memory message = bytes.concat(Gear.validatorsCommitmentHash(commitment));
        router.commitValidators(commitment, Gear.SignatureType.ECDSA, signBytes(_privateKeys, message));
    }

    function commitCode(uint256[] memory _privateKeys, Gear.CodeCommitment memory _commitment) internal {
        Gear.CodeCommitment[] memory _commitments = new Gear.CodeCommitment[](1);
        _commitments[0] = _commitment;
        commitCodes(_privateKeys, _commitments);
    }

    function commitCodes(uint256[] memory _privateKeys, Gear.CodeCommitment[] memory _commitments) internal {
        bytes memory _codesBytes;

        for (uint256 i = 0; i < _commitments.length; i++) {
            Gear.CodeCommitment memory _commitment = _commitments[i];
            _codesBytes = bytes.concat(_codesBytes, keccak256(abi.encodePacked(_commitment.id, _commitment.valid)));
        }

        router.commitCodes(_commitments, Gear.SignatureType.ECDSA, signBytes(_privateKeys, _codesBytes));
    }

    function commitBlock(uint256[] memory _privateKeys, Gear.BlockCommitment memory _commitment) internal {
        Gear.BlockCommitment[] memory _commitments = new Gear.BlockCommitment[](1);
        _commitments[0] = _commitment;
        commitBlocks(_privateKeys, _commitments);
    }

    function commitBlocks(uint256[] memory _privateKeys, Gear.BlockCommitment[] memory _commitments) internal {
        bytes memory _message;

        for (uint256 i = 0; i < _commitments.length; i++) {
            Gear.BlockCommitment memory _commitment = _commitments[i];
            _message = bytes.concat(_message, blockCommitmentHash(_commitment));
        }

        router.commitBlocks(_commitments, Gear.SignatureType.ECDSA, signBytes(_privateKeys, _message));
    }

    function blockCommitmentHash(Gear.BlockCommitment memory _commitment) internal pure returns (bytes32) {
        bytes memory _transitionsHashesBytes;

        for (uint256 i = 0; i < _commitment.transitions.length; i++) {
            Gear.StateTransition memory _transition = _commitment.transitions[i];

            bytes memory _valueClaimsBytes;
            for (uint256 j = 0; j < _transition.valueClaims.length; j++) {
                _valueClaimsBytes = bytes.concat(_valueClaimsBytes, Gear.valueClaimBytes(_transition.valueClaims[j]));
            }

            bytes memory _messagesHashesBytes;
            for (uint256 j = 0; j < _transition.messages.length; j++) {
                _messagesHashesBytes = bytes.concat(_messagesHashesBytes, Gear.messageHash(_transition.messages[j]));
            }

            _transitionsHashesBytes = bytes.concat(
                _transitionsHashesBytes,
                Gear.stateTransitionHash(
                    _transition.actorId,
                    _transition.newStateHash,
                    _transition.inheritor,
                    _transition.valueToReceive,
                    keccak256(_valueClaimsBytes),
                    keccak256(_messagesHashesBytes)
                )
            );
        }

        return Gear.blockCommitmentHash(
            _commitment.hash,
            _commitment.timestamp,
            _commitment.previousCommittedBlock,
            _commitment.predecessorBlock,
            keccak256(_transitionsHashesBytes)
        );
    }

    function signBytes(uint256[] memory _privateKeys, bytes memory _message)
        internal
        view
        returns (bytes[] memory signatures)
    {
        signatures = new bytes[](_privateKeys.length);
        bytes32 _messageHash = address(router).toDataWithIntendedValidatorHash(abi.encodePacked(keccak256(_message)));

        for (uint256 i = 0; i < _privateKeys.length; i++) {
            uint256 _key = _privateKeys[i];
            (uint8 v, bytes32 r, bytes32 s) = vm.sign(_key, _messageHash);
            signatures[i] = abi.encodePacked(r, s, v);
        }
    }

    function createOperator(address _operator) internal {
        vm.startPrank(_operator);
        {
            operatorRegistry.registerOperator();
            operatorNetworkOptInService.optIn(address(middleware));
            middleware.registerOperator();
        }
        vm.stopPrank();
    }

    function newVault(uint48 _epochDuration, address _operator) internal returns (address _vault) {
        address[] memory networkLimitSetRoleHolders = new address[](1);
        networkLimitSetRoleHolders[0] = _operator;

        (_vault,,) = vaultConfigurator.create(
            IVaultConfigurator.InitParams({
                version: 1,
                owner: _operator,
                vaultParams: abi.encode(
                    IVault.InitParams({
                        collateral: address(wrappedVara),
                        burner: address(middleware),
                        epochDuration: _epochDuration,
                        depositWhitelist: false,
                        isDepositLimit: false,
                        depositLimit: 0,
                        defaultAdminRoleHolder: _operator,
                        depositWhitelistSetRoleHolder: _operator,
                        depositorWhitelistRoleHolder: _operator,
                        isDepositLimitSetRoleHolder: _operator,
                        depositLimitSetRoleHolder: _operator
                    })
                ),
                delegatorIndex: 2,
                delegatorParams: abi.encode(
                    IOperatorSpecificDelegator.InitParams({
                        baseParams: IBaseDelegator.BaseParams({
                            defaultAdminRoleHolder: _operator,
                            hook: address(0),
                            hookSetRoleHolder: _operator
                        }),
                        networkLimitSetRoleHolders: networkLimitSetRoleHolders,
                        operator: _operator
                    })
                ),
                withSlasher: true,
                slasherIndex: 1,
                slasherParams: abi.encode(
                    IVetoSlasher.InitParams({
                        baseParams: IBaseSlasher.BaseParams({isBurnerHook: false}),
                        vetoDuration: eraDuration / 2,
                        resolverSetEpochsDelay: 3
                    })
                )
            })
        );
    }

    // Custom block hash implementation - is using for simulation of block chain in tests.

    function rollBlocks(uint256 _blocks) internal {
        uint256 _blockNumber = vm.getBlockNumber();
        uint256 _blockTimestamp = vm.getBlockTimestamp();
        for (uint256 i = 0; i < _blocks; i++) {
            _blockNumber += 1;
            _blockTimestamp += blockDuration;
            vm.roll(_blockNumber);
            setBlockhash(_blockNumber);
            vm.warp(_blockTimestamp);
        }
    }

    function setBlockhash(uint256 _blockNumber) internal {
        vm.setBlockhash(_blockNumber, blockHash(_blockNumber));
    }

    function blockHash(uint256 _blockNumber) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(_blockNumber));
    }
}
