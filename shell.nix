{
  pins ? import ./npins,
  pkgs ? import pins.nixpkgs { },
  lib ? pkgs.lib,

  # FIXME: `rustfilt` was removed in recent Nixpkgs due to
  #        lack of maintainance, so we need an old one.
  oldPkgs ? import pins.nixpkgs-old { },
}:
pkgs.mkShell.override { stdenv = pkgs.llvmPackages.stdenv; } {
  # FIXME: needed to build `jemalloc`
  CFLAGS = "-Wno-int-conversion";

  LIBCLANG_PATH = lib.makeLibraryPath [ pkgs.llvmPackages.libclang ];

  packages = [
    # Nix inputs manager
    pkgs.npins

    # Rust toolchain installer.
    #
    # FIXME: It's currently impossible to manage project's Rust
    #        toolchains with Nix due to hard lock on `rustup`.
    pkgs.rustup

    # Command runner
    pkgs.just

    # Maintainance tools and script dependencies
    pkgs.jq
    pkgs.just
    pkgs.typos

    # Build tools
    pkgs.protobuf
    pkgs.binaryen
    pkgs.foundry
    pkgs.cmake
    pkgs.perl
    pkgs.pkg-config

    # Testing tools
    pkgs.cargo-nextest
    pkgs.cargo-hack

    # Fuzzing tools
    pkgs.cargo-fuzz
    pkgs.cargo-binutils
    oldPkgs.rustfilt
  ];
}
