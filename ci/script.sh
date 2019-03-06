# This script takes care of testing your crate

set -ex

main() {
    cross build --target $TARGET
    cross build --target $TARGET --release

    if [ ! -z $DISABLE_TESTS ]; then
        return
    fi

    if [ "$TARGET" = "x86_64-apple-darwin" ] && [ "$TRAVIS_OS_NAME" = osx ]; then
        cargo test
        cargo test --release
    else
        local skip=()
        for s in ${SKIP_TESTS:-test_resolve_consistency}; do
            if [ -n $s ]; then skip=("${skip[@]}" --skip $s); fi
        done
        cross test --target $TARGET -- "${skip[@]}"
        cross test --target $TARGET --release -- "${skip[@]}"
    fi
}

# we don't run the "test phase" when doing deploys
if [ -z $TRAVIS_TAG ]; then
    main
fi
