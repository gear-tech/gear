{
  pins ? import ./npins,
  pkgs ? import pins.nixpkgs { },
  lib ? pkgs.lib,

  # FIXME: `rustfilt` was removed in recent Nixpkgs due to
  #        lack of maintenance, so we need an old one.
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

    # Maintenance tools and script dependencies
    pkgs.cargo-hakari
    pkgs.cargo-shear
    pkgs.jq
    pkgs.typos

    # Build tools
    pkgs.binaryen
    pkgs.cmake
    pkgs.foundry
    pkgs.nodejs
    pkgs.perl
    pkgs.pkg-config
    pkgs.protobuf

    # Testing tools
    pkgs.cargo-hack
    pkgs.cargo-nextest

    # Fuzzing tools
    pkgs.cargo-binutils
    pkgs.cargo-fuzz
    oldPkgs.rustfilt
  ];

  buildInputs = [
    pkgs.openssl
  ];
}
