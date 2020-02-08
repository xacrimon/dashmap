#!/bin/sh

RUSTFLAGS="-Zsanitizer=memory" cargo +nightly test -- --nocapture
RUSTFLAGS="-Zsanitizer=thread" cargo +nightly test -- --nocapture
