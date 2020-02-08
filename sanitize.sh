#!/bin/sh

RUSTFLAGS="-Zsanitizer=${1}" cargo +nightly test -- --nocapture
