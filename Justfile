FUZZER_NAME := 'diffray'
PROJECT_DIR := absolute_path(".")
PROFILE := 'dev'
PROFILE_DIR := 'debug'
CARGO_TARGET_DIR := env("CARGO_TARGET_DIR", "target")
FUZZER := CARGO_TARGET_DIR / PROFILE_DIR / FUZZER_NAME
LIBAFL_CC := CARGO_TARGET_DIR / PROFILE_DIR / "libafl_cc"
APP := 'libpng'

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
	{{FUZZER}} -vv -o run/diffs -i run/seeds/ -l ./run/log/ --mode single-cov --prog-a "/home/user/workspace/workshop/diffray/app/bins/.bin/GM/IM" --prog-b "/home/user/workspace/workshop/diffray/app/bins/.bin/GM/GM" --persistent-mode auto --simple-monitor auto

pre_run app=APP: 
	rm -rf ./run/ && rm -rf .cur_input_* && mkdir -p run && mkdir -p run/seeds && mkdir -p run/log
	# cp /home/user/workspace/workshop/diffray/misc/ssl/* run/seeds/
	cp ./app/corpus/png/* run/seeds/

docker_build:
	./docker/deploy.sh --target compile build diffray:latest

docker_save:
	./docker/deploy.sh save diffray:latest ./docker/compile/.tmp/diffray_latest.tar.gz

docker_transfer remote_host="kaichen_server":
	./docker/deploy.sh scp ./docker/compile/.tmp/diffray_latest.tar.gz {{remote_host}}:/home/jiangjy/download/diffray_compile.tar.gz

docker_load remote_host="kaichen_server":
	./docker/deploy.sh load {{remote_host}}:/home/jiangjy/download/diffray_compile.tar.gz

docker_start remote_host="kaichen_server":
	./docker/deploy.sh run {{remote_host}} --image diffray:latest --name jiangjy-diffray --port 27001 --volume /home/jiangjy/workshop/diffray-task:/root/diffray-task

docker_add_key remote_host="kaichen_server":
	./docker/deploy.sh add-key {{remote_host}} jiangjy-diffray ~/.ssh/id_rsa.pub




edocker:
	docker run --rm -it \
	  -v "$PWD":/home/dev/workspace \
	  -w /home/dev/workspace \
	  --net=host \
	  diffray:latest

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
