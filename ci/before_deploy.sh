# This script takes care of building your crate and packaging it for release

set -ex

main() {
    local src=$(pwd) \
          binary=px \
          stage=

    case $TRAVIS_OS_NAME in
        linux)
            stage=$(mktemp -d)
            ;;
        osx)
            stage=$(mktemp -d -t tmp)
            ;;
    esac

    case "$TARGET" in 
      *windows*) binary="px.exe" ;;
    esac

    test -f Cargo.lock || cargo generate-lockfile

    cross rustc --package pax --bin px --target $TARGET --release -- -C lto
    
    cp target/$TARGET/release/$binary $stage/

    cd $stage
    tar czf $src/chromy-$CRATE_NAME-$TRAVIS_TAG-$TARGET.tar.gz *
    cd $src

    rm -rf $stage
}

main
