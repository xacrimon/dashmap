#!/bin/sh

RUSTFLAGS="-Z sanitizer=thread" cargo +nightly test -- --nocapture
