FUZZER_NAME := 'diffray'
PROJECT_DIR := absolute_path(".")
PROFILE := 'release'
PROFILE_DIR := 'release'
CARGO_TARGET_DIR := env("CARGO_TARGET_DIR", "target")
FUZZER := CARGO_TARGET_DIR / PROFILE_DIR / FUZZER_NAME
LIBAFL_CC := CARGO_TARGET_DIR / PROFILE_DIR / "libafl_cc"
APP := 'png'

alias build := fuzzer

load_app app=APP:
	rm -rf ./libdifferential.a
	cp "./app/differential_libs/{{app}}/libdifferential_{{app}}.a" ./libdifferential.a

cc:
    cargo build --profile={{PROFILE}} --bin libafl_cc
cxx:
    cargo build --profile={{PROFILE}} --bin libafl_cxx

fuzzer app=APP: (load_app app) cc cxx
	cargo build --profile={{PROFILE}}

run app=APP: (fuzzer app) (pre_run app)
	{{FUZZER}} -t 60 -c -o run/diffs -i run/seeds/ --corpus run/corpus -v -l ./run/log/log.txt --cov-mode none --rv-mode single

run_directly app=APP: (pre_run app)
	{{FUZZER}} -t 60 -c -o run/diffs -i run/seeds/ --corpus run/corpus/ -v -l ./run/log/log.txt --cov-mode max-min --rv-mode none

pre_run app=APP: 
	rm -rf ./run/ && mkdir -p run && mkdir -p run/seeds && mkdir -p run/log
	cp ./app/corpus/{{app}}/*.{{app}} run/seeds/

clean: clean_build clean_app clean_run

clean_build:
	cargo clean

clean_app:
	rm -rf ./libdifferential.a

clean_run:
	rm -rf ./run/
