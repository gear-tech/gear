// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

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
import {SigningKey, FROSTOffchain} from "frost-secp256k1-evm/FROSTOffchain.sol";
import {WrappedVara} from "../src/WrappedVara.sol";
import {IMirror, Mirror} from "../src/Mirror.sol";
import {IRouter, Router} from "../src/Router.sol";
import {IMiddleware} from "../src/IMiddleware.sol";
import {Middleware} from "../src/Middleware.sol";
import {Gear} from "../src/libraries/Gear.sol";

import {IDefaultStakerRewards} from "symbiotic-rewards/src/interfaces/defaultStakerRewards/IDefaultStakerRewards.sol";
import {DefaultStakerRewards} from "symbiotic-rewards/src/contracts/defaultStakerRewards/DefaultStakerRewards.sol";
import {DefaultStakerRewardsFactory} from
    "symbiotic-rewards/src/contracts/defaultStakerRewards/DefaultStakerRewardsFactory.sol";
import {DefaultOperatorRewards} from "symbiotic-rewards/src/contracts/defaultOperatorRewards/DefaultOperatorRewards.sol";
import {DefaultOperatorRewardsFactory} from
    "symbiotic-rewards/src/contracts/defaultOperatorRewards/DefaultOperatorRewardsFactory.sol";
import {console} from "forge-std/console.sol";

contract Base is POCBaseTest {
    using MessageHashUtils for address;
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using FROSTOffchain for SigningKey;

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

    DefaultStakerRewardsFactory defaultStakerRewardsFactory;
    DefaultOperatorRewardsFactory defaultOperatorRewardsFactory;

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

        vm.startPrank(admin, admin);
        {
            IMiddleware.InitParams memory initParams = _defaultMiddlewareInitParams();
            middleware = Middleware(
                Upgrades.deployTransparentProxy(
                    "Middleware.sol", admin, abi.encodeCall(Middleware.initialize, (initParams))
                )
            );
        }
        vm.stopPrank();
    }

    function setUpRouter(Gear.AggregatedPublicKey memory _aggregatedPublicKey, address[] memory _validators) internal {
        require(admin != address(0), "Base: admin must be initialized");
        require(address(wrappedVara) != address(0), "Base: wrappedVara should be initialized");
        require(eraDuration > 0, "Base: eraDuration should be greater than 0");
        require(electionDuration > 0, "Base: electionDuration should be greater than 0");
        require(blockDuration > 0, "Base: blockDuration should be greater than 0");

        address wrappedVaraAddress = address(wrappedVara);

        address mirrorAddress = vm.computeCreateAddress(admin, vm.getNonce(admin) + 2);

        // Here nonce is 7 because of extra 4 calls in the _defaultMiddlewareInitParams
        address middlewareAddress = vm.computeCreateAddress(admin, vm.getNonce(admin) + 7);
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
                            wrappedVaraAddress,
                            middlewareAddress,
                            uint256(eraDuration),
                            uint256(electionDuration),
                            uint256(validationDelay),
                            _aggregatedPublicKey,
                            "",
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
            mirror = new Mirror(address(router));
        }
        vm.stopPrank();

        assertEq(router.mirrorImpl(), address(mirror));
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
        _vault = newVault(_operator, _defaultVaultInitParams(_operator));
        address _rewards = newStakerRewards(_vault, _operator);

        vm.startPrank(_operator);
        {
            middleware.registerVault(_vault, _rewards);
            operatorVaultOptInService.optIn(_vault);
            IOperatorSpecificDelegator(IVault(_vault).delegator()).setNetworkLimit(
                middleware.subnetwork(), type(uint256).max
            );
        }
        vm.stopPrank();
    }

    function commitValidators(uint256[] memory _privateKeys, Gear.ValidatorsCommitment memory commitment) internal {
        bytes memory message = bytes.concat(Gear.validatorsCommitmentHash(commitment));
        router.commitValidators(commitment, Gear.SignatureType.FROST, signBytes(_privateKeys, message));
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
            _codesBytes = bytes.concat(_codesBytes, Gear.codeCommitmentHash(_commitment));
        }

        router.commitBatch(
            Gear.BatchCommitment({
                codeCommitments: _commitments,
                blockCommitments: new Gear.BlockCommitment[](0),
                rewardCommitments: new Gear.RewardsCommitment[](0)
            }),
            Gear.SignatureType.FROST,
            signBytes(_privateKeys, abi.encodePacked(keccak256(""), keccak256(_codesBytes), keccak256("")))
        );
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

        router.commitBatch(
            Gear.BatchCommitment({
                codeCommitments: new Gear.CodeCommitment[](0),
                blockCommitments: _commitments,
                rewardCommitments: new Gear.RewardsCommitment[](0)
            }),
            Gear.SignatureType.FROST,
            signBytes(_privateKeys, abi.encodePacked(keccak256(_message), keccak256(""), keccak256("")))
        );
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
                    _transition.exited,
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

    // TODO: add SignatureType as param here
    function signBytes(uint256[] memory _privateKeys, bytes memory _message)
        internal
        returns (bytes[] memory signatures)
    {
        signatures = new bytes[](1);
        bytes32 _messageHash = address(router).toDataWithIntendedValidatorHash(abi.encodePacked(keccak256(_message)));
        SigningKey signingKey = FROSTOffchain.signingKeyFromScalar(_privateKeys[0]);
        (uint256 signatureRX, uint256 signatureRY, uint256 signatureZ) = signingKey.createSignature(_messageHash);
        signatures[0] = abi.encodePacked(signatureRX, signatureRY, signatureZ);
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

    function newVault(address _operator, IVault.InitParams memory _vaultParams) internal returns (address _vault) {
        address[] memory networkLimitSetRoleHolders = new address[](1);
        networkLimitSetRoleHolders[0] = _operator;

        (_vault,,) = vaultConfigurator.create(
            IVaultConfigurator.InitParams({
                version: 1,
                owner: _operator,
                vaultParams: abi.encode(_vaultParams),
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

    function newStakerRewards(address _vault, address _operator) internal returns (address _rewards) {
        _rewards = defaultStakerRewardsFactory.create(
            IDefaultStakerRewards.InitParams({
                vault: _vault,
                adminFee: 10000,
                defaultAdminRoleHolder: _operator,
                adminFeeClaimRoleHolder: _operator,
                adminFeeSetRoleHolder: _operator
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

    // This function is used to create default vault params for tests and ability to override them in tests
    function _defaultVaultInitParams(address _operator)
        internal
        view
        returns (IVault.InitParams memory defaultVaultParams)
    {
        defaultVaultParams = IVault.InitParams({
            collateral: address(wrappedVara),
            burner: address(middleware),
            epochDuration: eraDuration * 2,
            depositWhitelist: false,
            isDepositLimit: false,
            depositLimit: 0,
            defaultAdminRoleHolder: _operator,
            depositWhitelistSetRoleHolder: _operator,
            depositorWhitelistRoleHolder: _operator,
            isDepositLimitSetRoleHolder: _operator,
            depositLimitSetRoleHolder: _operator
        });
    }

    function _defaultMiddlewareInitParams() internal returns (IMiddleware.InitParams memory params) {
        address defaultStakerRewards_ =
            address(new DefaultStakerRewards(address(vaultFactory), address(networkMiddlewareService)));
        defaultStakerRewardsFactory = new DefaultStakerRewardsFactory(defaultStakerRewards_);

        address defaultOperatorRewards_ = address(new DefaultOperatorRewards(address(networkMiddlewareService)));
        defaultOperatorRewardsFactory = new DefaultOperatorRewardsFactory(defaultOperatorRewards_);

        Gear.SymbioticRegistries memory registries = Gear.SymbioticRegistries({
            vaultRegistry: address(vaultFactory),
            operatorRegistry: address(operatorRegistry),
            networkRegistry: address(networkRegistry),
            middlewareService: address(networkMiddlewareService),
            networkOptIn: address(operatorNetworkOptInService),
            stakerRewardsFactory: address(defaultStakerRewardsFactory)
        });

        params = IMiddleware.InitParams({
            owner: admin,
            eraDuration: eraDuration,
            minVaultEpochDuration: eraDuration * 2,
            operatorGracePeriod: eraDuration * 2,
            vaultGracePeriod: eraDuration * 2,
            minVetoDuration: eraDuration / 3,
            minSlashExecutionDelay: eraDuration / 3,
            allowedVaultImplVersion: 1,
            vetoSlasherImplType: 1,
            maxResolverSetEpochsDelay: type(uint256).max,
            collateral: address(wrappedVara),
            maxAdminFee: 10000,
            operatorRewards: defaultOperatorRewardsFactory.create(),
            router: address(router),
            roleSlashRequester: admin,
            roleSlashExecutor: admin,
            vetoResolver: admin,
            registries: registries
        });
    }
}
