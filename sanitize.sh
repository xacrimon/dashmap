#!/bin/sh

RUSTFLAGS="-Zsanitizer=thread" cargo +nightly test -- --nocapture
