// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {IMiddleware} from "../src/IMiddleware.sol";
import {Middleware} from "../src/Middleware.sol";
import {Mirror} from "../src/Mirror.sol";
import {Router} from "../src/Router.sol";
import {WrappedVara} from "../src/WrappedVara.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {FROSTOffchain, SigningKey} from "frost-secp256k1-evm/FROSTOffchain.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {IVaultConfigurator} from "symbiotic-core/src/interfaces/IVaultConfigurator.sol";
import {IBaseDelegator} from "symbiotic-core/src/interfaces/delegator/IBaseDelegator.sol";
import {IOperatorSpecificDelegator} from "symbiotic-core/src/interfaces/delegator/IOperatorSpecificDelegator.sol";
import {IBaseSlasher} from "symbiotic-core/src/interfaces/slasher/IBaseSlasher.sol";
import {IVetoSlasher} from "symbiotic-core/src/interfaces/slasher/IVetoSlasher.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {POCBaseTest} from "symbiotic-core/test/POCBase.t.sol";

import {
    DefaultOperatorRewards
} from "symbiotic-rewards/src/contracts/defaultOperatorRewards/DefaultOperatorRewards.sol";
import {
    DefaultOperatorRewardsFactory
} from "symbiotic-rewards/src/contracts/defaultOperatorRewards/DefaultOperatorRewardsFactory.sol";
import {DefaultStakerRewards} from "symbiotic-rewards/src/contracts/defaultStakerRewards/DefaultStakerRewards.sol";
import {
    DefaultStakerRewardsFactory
} from "symbiotic-rewards/src/contracts/defaultStakerRewards/DefaultStakerRewardsFactory.sol";
import {IDefaultStakerRewards} from "symbiotic-rewards/src/interfaces/defaultStakerRewards/IDefaultStakerRewards.sol";

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
                payable(Upgrades.deployTransparentProxy(
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
                    ))
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
        (uint128 numerator, uint128 denominator) = router.signingThresholdFraction();
        assertEq(numerator, Gear.VALIDATORS_THRESHOLD_NUMERATOR);
        assertEq(denominator, Gear.VALIDATORS_THRESHOLD_DENOMINATOR);
        assertTrue(router.areValidators(_validators));
        assertEq(router.latestCommittedBatchHash(), bytes32(0));
        assertEq(router.latestCommittedBatchTimestamp(), uint48(0));
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
            IOperatorSpecificDelegator(IVault(_vault).delegator())
                .setNetworkLimit(middleware.subnetwork(), type(uint256).max);
        }
        vm.stopPrank();
    }

    function commitBatch(uint256[] memory _privateKeys, Gear.BatchCommitment memory _batch, bool revertExpected)
        internal
    {
        if (revertExpected) {
            vm.expectRevert();
        }
        router.commitBatch(_batch, Gear.SignatureType.FROST, signHash(_privateKeys, batchCommitmentHash(_batch)));
    }

    function commitBlock(uint256[] memory _privateKeys, Gear.StateTransition[] memory _transactions) internal {
        bytes32 _blockHash = blockHash(vm.getBlockNumber());
        uint48 _timestamp = uint48(vm.getBlockTimestamp());

        rollBlocks(1);

        commitBlock(_privateKeys, _transactions, _blockHash, _timestamp, false);
    }

    function commitBlock(
        uint256[] memory _privateKeys,
        Gear.StateTransition[] memory _transactions,
        bytes32 _blockHash,
        uint48 _timestamp,
        bool revertExpected
    ) internal {
        Gear.ChainCommitment memory _chainCommitment =
            Gear.ChainCommitment({transitions: _transactions, head: keccak256("head")});

        Gear.ChainCommitment[] memory _chainCommitments = new Gear.ChainCommitment[](1);
        _chainCommitments[0] = _chainCommitment;

        Gear.BatchCommitment memory _batch = Gear.BatchCommitment({
            blockHash: _blockHash,
            blockTimestamp: _timestamp,
            previousCommittedBatchHash: router.latestCommittedBatchHash(),
            expiry: 3,
            chainCommitment: _chainCommitments,
            codeCommitments: new Gear.CodeCommitment[](0),
            rewardsCommitment: new Gear.RewardsCommitment[](0),
            validatorsCommitment: new Gear.ValidatorsCommitment[](0)
        });

        commitBatch(_privateKeys, _batch, revertExpected);
    }

    function commitCode(uint256[] memory _privateKeys, Gear.CodeCommitment memory _commitment) internal {
        Gear.CodeCommitment[] memory _codeCommitments = new Gear.CodeCommitment[](1);
        _codeCommitments[0] = _commitment;

        Gear.BatchCommitment memory _batch = Gear.BatchCommitment({
            blockHash: blockHash(vm.getBlockNumber()),
            blockTimestamp: uint48(vm.getBlockTimestamp()),
            previousCommittedBatchHash: router.latestCommittedBatchHash(),
            expiry: 3,
            chainCommitment: new Gear.ChainCommitment[](0),
            codeCommitments: _codeCommitments,
            rewardsCommitment: new Gear.RewardsCommitment[](0),
            validatorsCommitment: new Gear.ValidatorsCommitment[](0)
        });

        rollBlocks(1);

        commitBatch(_privateKeys, _batch, false);
    }

    function commitValidators(
        uint256[] memory _privateKeys,
        Gear.ValidatorsCommitment memory _commitment,
        bool revertExpected
    ) internal {
        Gear.ValidatorsCommitment[] memory _validatorsCommitments = new Gear.ValidatorsCommitment[](1);
        _validatorsCommitments[0] = _commitment;

        Gear.BatchCommitment memory _batch = Gear.BatchCommitment({
            blockHash: blockHash(vm.getBlockNumber()),
            blockTimestamp: uint48(vm.getBlockTimestamp()),
            previousCommittedBatchHash: router.latestCommittedBatchHash(),
            expiry: 3,
            chainCommitment: new Gear.ChainCommitment[](0),
            codeCommitments: new Gear.CodeCommitment[](0),
            rewardsCommitment: new Gear.RewardsCommitment[](0),
            validatorsCommitment: _validatorsCommitments
        });

        rollBlocks(1);

        commitBatch(_privateKeys, _batch, revertExpected);
    }

    function batchCommitmentHash(Gear.BatchCommitment memory _batch) internal pure returns (bytes32) {
        require(_batch.rewardsCommitment.length == 0, "Base: rewardsCommitment is not supported yet");
        require(_batch.validatorsCommitment.length <= 1, "Base: validatorsCommitment length must be 0 or 1");
        require(_batch.chainCommitment.length <= 1, "Base: chainCommitment length must be 0 or 1");

        bytes32 _chainCommitmentHash;
        if (_batch.chainCommitment.length == 1) {
            _chainCommitmentHash = chainCommitmentHash(_batch.chainCommitment[0]);
        } else {
            _chainCommitmentHash = keccak256("");
        }

        bytes32 _codeCommitmentsHash = codeCommitmentsHash(_batch.codeCommitments);

        bytes32 _validatorsCommitmentHash;
        if (_batch.validatorsCommitment.length == 1) {
            _validatorsCommitmentHash = Gear.validatorsCommitmentHash(_batch.validatorsCommitment[0]);
        } else {
            _validatorsCommitmentHash = keccak256("");
        }

        bytes32 _rewardsCommitmentHash = keccak256("");

        return Gear.batchCommitmentHash(
            _batch.blockHash,
            _batch.blockTimestamp,
            _batch.previousCommittedBatchHash,
            _batch.expiry,
            _chainCommitmentHash,
            _codeCommitmentsHash,
            _rewardsCommitmentHash,
            _validatorsCommitmentHash
        );
    }

    function chainCommitmentHash(Gear.ChainCommitment memory _commitment) internal pure returns (bytes32) {
        bytes32[] memory _transitionsHashes = new bytes32[](_commitment.transitions.length);
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

            _transitionsHashes[i] = Gear.stateTransitionHash(
                _transition.actorId,
                _transition.newStateHash,
                _transition.exited,
                _transition.inheritor,
                _transition.valueToReceive,
                _transition.valueToReceiveNegativeSign,
                keccak256(_valueClaimsBytes),
                keccak256(_messagesHashesBytes)
            );
        }

        return Gear.chainCommitmentHash(keccak256(abi.encodePacked(_transitionsHashes)), _commitment.head);
    }

    function codeCommitmentsHash(Gear.CodeCommitment[] memory _commitments) internal pure returns (bytes32) {
        bytes32[] memory _codeCommitmentHashes = new bytes32[](_commitments.length);
        for (uint256 i = 0; i < _commitments.length; i++) {
            _codeCommitmentHashes[i] = Gear.codeCommitmentHash(_commitments[i]);
        }

        return keccak256(abi.encodePacked(_codeCommitmentHashes));
    }

    // TODO: add SignatureType as param here
    function signHash(uint256[] memory _privateKeys, bytes32 _hash) internal returns (bytes[] memory signatures) {
        signatures = new bytes[](1);
        bytes32 _messageHash = address(router).toDataWithIntendedValidatorHash(abi.encodePacked(_hash));
        SigningKey signingKey = FROSTOffchain.signingKeyFromScalar(_privateKeys[0]);
        (uint256 signatureCommitmentX, uint256 signatureCommitmentY, uint256 signatureZ) =
            signingKey.createSignature(_messageHash);
        signatures[0] = abi.encodePacked(signatureCommitmentX, signatureCommitmentY, signatureZ);
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
                            defaultAdminRoleHolder: _operator, hook: address(0), hookSetRoleHolder: _operator
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

    function rollOneBlockAndWarp(uint256 _timestamp) internal {
        require(_timestamp > vm.getBlockTimestamp(), "Base: timestamp must be greater than current block timestamp");
        uint256 _blockNumber = vm.getBlockNumber() + 1;
        vm.warp(_timestamp);
        vm.roll(_blockNumber);
        setBlockhash(_blockNumber);
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

        Gear.SymbioticContracts memory symbiotic = Gear.SymbioticContracts({
            vaultRegistry: address(vaultFactory),
            operatorRegistry: address(operatorRegistry),
            networkRegistry: address(networkRegistry),
            middlewareService: address(networkMiddlewareService),
            networkOptIn: address(operatorNetworkOptInService),
            stakerRewardsFactory: address(defaultStakerRewardsFactory),
            operatorRewards: defaultOperatorRewardsFactory.create(),
            roleSlashRequester: admin,
            roleSlashExecutor: admin,
            vetoResolver: admin
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
            router: address(router),
            symbiotic: symbiotic
        });
    }
}
