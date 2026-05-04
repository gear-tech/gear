#!/bin/bash
#
# Inject `README.md` into `src/lib.rs`
readonly ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"
readonly README="${ROOT_DIR}/README.md"
readonly LIB_RS="${ROOT_DIR}/src/lib.rs"

############################################
# Concat `README.md` and `src/lib.rs`
############################################
function main() {
    readme=$(cat ${README} | sed 's/^/\/\/\!/' | sed 's/\!\(\S\)/\! \1/')
    lib_rs=$(cat ${LIB_RS} | sed '/\/\/\!/c\')
    echo -e "${readme}\n${lib_rs}" > "${LIB_RS}"
}

main
