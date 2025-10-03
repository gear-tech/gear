import 'lib.just'

[private]
default:
    @just --list ethexe --unsorted

# Run pre-commit checks for `ethexe`
pre-commit: contracts-pre-commit no-contracts-pre-commit

# Run pre-commit checks for `ethexe` contracts
contracts-pre-commit: (ensure-binary "forge" '''
    curl -L https://foundry.paradigm.xyz | bash
    foundryup
''')
    # Clean contracts
    forge clean --root ethexe/contracts
    # Formatting contracts
    forge fmt --root ethexe/contracts
    # Building contracts
    forge build --root ethexe/contracts
    # Testing contracts
    forge test --root ethexe/contracts -vvv
    # Copying Middleware artifact
    cp ./ethexe/contracts/out/Middleware.sol/Middleware.json ./ethexe/ethereum
    # Copying Mirror artifact
    cp ./ethexe/contracts/out/Mirror.sol/Mirror.json ./ethexe/ethereum
    # Copying Router artifact
    cp ./ethexe/contracts/out/Router.sol/Router.json ./ethexe/ethereum
    # Copying TransparentUpgradeableProxy artifact
    cp ./ethexe/contracts/out/TransparentUpgradeableProxy.sol/TransparentUpgradeableProxy.json ./ethexe/ethereum
    # Copying WrappedVara artifact
    cp ./ethexe/contracts/out/WrappedVara.sol/WrappedVara.json ./ethexe/ethereum

# Run pre-commit checks for `ethexe` excluding contracts
no-contracts-pre-commit:
    # Formatting ethexe
    cargo fmt --all
    # Clippy checking ethexe
    cargo clippy -p "ethexe-*" --all-targets --all-features -- --no-deps -D warnings
    # Testing ethexe
    cargo nextest run -p "ethexe-*" --no-fail-fast


