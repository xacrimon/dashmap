#!/bin/sh

RUSTFLAGS="-Z sanitizer=${1}" cargo +nightly test -- --nocapture
