FUZZER_NAME := 'diffray'

# Portable competition entrypoints; each run keeps its own output directory.
contest-build:
    python3 scripts/contest_demo.py build

contest-demo seconds="30": contest-build
    python3 scripts/contest_demo.py run --seconds {{quote(seconds)}}

contest-verify:
    python3 scripts/contest_demo.py verify

contest-replay input="examples/contest/known_difference.txt":
    python3 scripts/contest_demo.py replay {{quote(input)}}

real-json-build:
    python3 scripts/real_json_demo.py build

real-json-demo seconds="30": real-json-build
    python3 scripts/real_json_demo.py run --seconds {{quote(seconds)}}

real-json-verify:
    python3 scripts/real_json_demo.py verify

real-json-replay input="examples/real-json/known-difference.json":
    python3 scripts/real_json_demo.py replay {{quote(input)}}

PROJECT_DIR := absolute_path(".")
PROFILE := 'dev'
PROFILE_DIR := 'debug'
CARGO_TARGET_DIR := env("CARGO_TARGET_DIR", "target")
FUZZER := CARGO_TARGET_DIR / PROFILE_DIR / FUZZER_NAME
LIBAFL_CC := CARGO_TARGET_DIR / PROFILE_DIR / "libafl_cc"
APP := 'libpng'
APP_FOLDER := PROJECT_DIR / "app" 

alias build := fuzzer

load_app app=APP:
	rm -rf ./libdifferential.a
	cp "./app/differential_libs/libs/libdifferential_{{app}}.a" ./libdifferential.a

cc:
    cargo build --profile={{PROFILE}} --bin libafl_cc
cxx:
    cargo build --profile={{PROFILE}} --bin libafl_cxx

fuzzer app=APP: cc cxx
	cargo build --profile={{PROFILE}}

run app=APP: (fuzzer app) (pre_run app)
	{{FUZZER}} -vvv -o run/diffs -i run/seeds/ --corpus run/corpus/ -l ./run/log/ --cov-mode none --rv-mode single --prog-a "/home/user/workspace/workshop/diffray/app/bins/.bin/libspng/libpng" --prog-b "/home/user/workspace/workshop/diffray/app/bins/.bin/libspng/libspng"

run_directly app=APP: (pre_run app)
	{{FUZZER}} -vv -o run/diffs -i run/seeds/ -l ./run/log/ --mode single-cov --prog-a "/home/user/workspace/diffray/app/bins/.bin/cjson/cjson" --prog-b "/home/user/workspace/diffray/app/bins/.bin/cjson/mjson" --persistent-mode auto --simple-monitor auto

replay app=APP: (pre_run app)
	{{FUZZER}} -vv -o run/diffs -i /home/user/workspace/diffray/LocateFix/data/collected/by_program/png/aflplusplus/rep_01_20260114T065840Z_long_fuzz/queue -l ./run/log/ --mode replay-cov --prog-a "/home/user/workspace/diffray/app/bins/.bin/libpng/libpng" --prog-b "/home/user/workspace/diffray/app/bins/.bin/libpng/libspng" --persistent-mode auto --simple-monitor auto 

pre_run app=APP: 
	rm -rf ./run/ && rm -rf .cur_input_* && mkdir -p run && mkdir -p run/seeds && mkdir -p run/log
	# cp /home/user/workspace/workshop/diffray/misc/ssl/* run/seeds/
	cp ./app/corpus/json/* run/seeds/
	# cp /home/user/workspace/diffray/AFLplusplus/quick/run/out/default/queue/id* run/seeds/
	# cp /home/user/workspace/diffray/misc/corpus_tmp/fuzz-corpora/x509/* run/seeds/

docker_build:
	./docker/deploy.sh --target compile build diffray:latest

docker_save:
	./docker/deploy.sh save diffray:latest ./docker/compile/.tmp/diffray_latest.tar.gz

docker_transfer remote_host="kaichen_server":
	./docker/deploy.sh scp ./docker/compile/.tmp/diffray_latest.tar.gz {{remote_host}}:/home/jiangjy/download/diffray_compile.tar.gz

docker_load remote_host="kaichen_server":
	./docker/deploy.sh load {{remote_host}}:/home/jiangjy/download/diffray_compile.tar.gz

docker_start remote_host="Chengshuaibing":
	./docker/deploy.sh run {{remote_host}} --image diffray:latest --name jiangjy-diffray --port 27001 --volume /home/ChengShuaiBing/software/diffray-task:/root/diffray-task --target-volume none --run-volume none --cargo-volume cargo_cache

docker_add_key remote_host="Chengshuaibing":
	./docker/deploy.sh add-key {{remote_host}} jiangjy-diffray ~/.ssh/id_rsa.pub

scp_fuzzer remote_host="diffray_container":
	scp {{FUZZER}} {{remote_host}}:/root/diffray-task/fuzzer/{{FUZZER_NAME}}



recompile_app app:
    #!/bin/bash
    # cd {{APP_FOLDER}}/script/ && just {{app}}
    cd {{APP_FOLDER}}/bins/{{app}} && ./gen.sh


run_docker_diffray:
	# keep running in background
	docker run -d \
	  -v "$PWD":/home/dev/workspace \
	  -w /home/dev/workspace \
	  --net=host \
	  --privileged \
	  --user root \
	  -e START_SSHD=1 \
	  --name diffray_container \
	  diffray:latest


edocker:
	docker exec -it diffray_container /bin/bash

edocker_afl:
	docker run --rm -it \
	  -v "$PWD":/home/dev/workspace \
	  -w /home/dev/workspace \
	  --net=host \
	  fuzzguard:latest /bin/bash

profile: clean pre_run
	RUSTFLAGS="-C debuginfo=2" cargo build --profile=release
	cargo flamegraph --release --bin {{FUZZER_NAME}} -- -vv -o run/diffs -i run/seeds/ -l ./run/log/ --mode single-cov --prog-a "/home/user/workspace/workshop/diffray/app/bins/.bin/cjson/cjson" --prog-b "/home/user/workspace/workshop/diffray/app/bins/.bin/cjson/mjson" --persistent-mode auto --simple-monitor auto


clean: clean_build clean_app clean_run

clean_build:
	cargo clean

clean_app:
	rm -rf ./libdifferential.a

clean_run:
	rm -rf ./run/
	rm -rf ./.cur_input_*
