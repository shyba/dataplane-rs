.PHONY: all native test clean

all: native

native:
	CFLAGS="-fpermissive" cargo build --release --manifest-path native/Cargo.toml
	mkdir -p priv
	cp ../../../target/release/libranch_uring_nif.so priv/ranch_uring_nif.so

test:
	rebar3 ct

clean:
	rebar3 clean
	cargo clean --manifest-path native/Cargo.toml
