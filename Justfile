FUZZER_NAME := 'diffray'
PROJECT_DIR := absolute_path(".")
PROFILE := 'release'
PROFILE_DIR := 'release'
CARGO_TARGET_DIR := env("CARGO_TARGET_DIR", "target")
FUZZER := CARGO_TARGET_DIR / PROFILE_DIR / FUZZER_NAME
LIBAFL_CC := CARGO_TARGET_DIR / PROFILE_DIR / "libafl_cc"


alias build := fuzzer

cc:
    cargo build --profile={{PROFILE}} --bin libafl_cc
cxx:
    cargo build --profile={{PROFILE}} --bin libafl_cxx

fuzzer: cc cxx
	cargo build --profile={{PROFILE}}

run: fuzzer
	rm -rf crashes
	{{FUZZER}} -t 10 -i seeds/

run_directly:
	rm -rf crashes
	{{FUZZER}} -t 60 -c -i seeds/

clean:
	cargo clean
