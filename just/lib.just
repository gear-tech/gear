[private]
ensure-binary binary hint: (
    ensure
    ("command -v " + binary)
    ("`" + binary + "` program")
    hint
)

[private]
ensure-cargo subcommand: (
    ensure
    ("cargo --list | awk '{ print $1 }' | grep '^" + subcommand + "$'")
    ("`cargo " + subcommand + "` subcommand")
    ("cargo install cargo-" + subcommand)
)

[private]
ensure condition what hint:
    @if ! ({{ condition }}) >/dev/null; then \
        echo 'You need {{ what }} to run this script.' >&2 ;\
        echo >&2 ;\
        echo 'To install it, run following command:' >&2 ;\
        echo '{{ replace_regex(trim(hint), "^|\n", "${0}> ") }}' >&2 ;\
        echo >&2 ;\
        exit 1 ;\
    fi

