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

run: fuzzer pre_run
	{{FUZZER}} -t 60 -c -o run/diffs -i run/seeds/ -v -l ./run/log/log.txt

run_directly: pre_run
	{{FUZZER}} -t 60 -c -o run/diffs -i run/seeds/ -v -l ./run/log/log.txt

pre_run:
	rm -rf ./run/ && mkdir -p run && mkdir -p run/seeds && mkdir -p run/log
	cp ./app/corpus/png/*.png run/seeds/

clean:
	cargo clean
