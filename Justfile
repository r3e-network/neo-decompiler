default: fmt test

fmt:
	cargo fmt

fmt-check:
	cargo fmt --all -- --check

test:
	cargo test

test-no-default:
	cargo test --no-default-features

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

clippy-no-default:
	cargo clippy --all-targets --no-default-features -- -D warnings

package:
	cargo package --allow-dirty --no-verify

doc:
	cargo doc --no-deps

doc-check:
	RUSTDOCFLAGS='-D warnings' cargo doc --no-deps

msrv:
	cargo +1.83.0 test
	cargo +1.83.0 test --no-default-features

ci: fmt-check test clippy test-no-default clippy-no-default doc-check msrv

artifact-sweep:
	bash tools/ci/artifact_sweep.sh
