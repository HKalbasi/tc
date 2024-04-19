let
  nixpkgs = import <nixpkgs> { };
  inherit (nixpkgs) pkgs;
  stdenv = pkgs.clangStdenv;
in
stdenv.mkDerivation {
  name = "tc";
  buildInputs = [
    pkgs.clang
    pkgs.cmake
    pkgs.llvm
    pkgs.libllvm
    pkgs.openssl
    pkgs.pkgconfig
    pkgs.python3
    pkgs.rustup
    pkgs.zlib
    pkgs.z3
    pkgs.mold
  ];
  LLVM_SYS_170_PREFIX = "/home/user/oss/llvm-project-main/build";
  LD_LIBRARY_PATH = "/home/user/oss/llvm-project-main/build/lib";
}
