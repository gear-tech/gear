{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, flake-utils, nixpkgs, rust-overlay}:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      in {
        toolchain.extensions = [
          "rust-src"
          "rust-analyzer"
          "llvm-tools"
        ];
      
        devShells.default = with pkgs; mkShell {
          CFLAGS="-Wno-int-conversion";
          CC = "clang";
          IN_NIX_SHELL = "flake";
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          packages = [        
            toolchain
            protobuf
            rocksdb
            llvmPackages.clang
            llvmPackages.libclang
            jemalloc
            binaryen
            foundry
            cmake
            git
            cargo-nextest                        
            cargo-hack
            cargo-fuzz
            cargo-binutils
          ];
        };
      }
    );
}
