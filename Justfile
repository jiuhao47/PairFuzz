# Competition entrypoints. Each fuzzing run keeps its own output directory.
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

real-json-verify: real-json-build
    python3 scripts/real_json_demo.py verify

real-json-replay input="examples/real-json/known-difference.json":
    python3 scripts/real_json_demo.py replay {{quote(input)}}

performance-build: real-json-build

performance-smoke cpu: performance-build
    python3 scripts/performance_campaign.py run --cpu {{quote(cpu)}} --warmup-seconds 1 --trial-seconds 5 --trials 1

performance-baseline cpu: performance-build
    python3 scripts/performance_campaign.py run --cpu {{quote(cpu)}}

real-targets-fetch pair="all":
    python3 scripts/real_targets.py fetch --pair {{quote(pair)}}

real-targets-build pair="png" profile="asan":
    python3 scripts/real_targets.py build --pair {{quote(pair)}} --profile {{quote(profile)}}

real-targets-verify pair="png" profile="asan":
    python3 scripts/real_targets.py verify --pair {{quote(pair)}} --profile {{quote(profile)}}

real-targets-verify-all profile manifest_map:
    python3 scripts/real_targets.py verify --pair all --profile {{quote(profile)}} --manifest-map {{quote(manifest_map)}}

real-targets-run pair="png" seconds="30" cpu="0" profile="release" seed="138977575630630":
    python3 scripts/real_targets.py run --pair {{quote(pair)}} --profile {{quote(profile)}} --seconds {{quote(seconds)}} --cpu {{quote(cpu)}} --seed {{quote(seed)}}

real-targets-replay pair input manifest:
    python3 scripts/real_targets.py replay --pair {{quote(pair)}} --input {{quote(input)}} --build-manifest {{quote(manifest)}}

real-targets-x509-run seconds="30" cpu="0" profile="release" seed="138977575630630":
    python3 scripts/real_targets.py run --pair x509 --profile {{quote(profile)}} --seconds {{quote(seconds)}} --cpu {{quote(cpu)}} --seed {{quote(seed)}}

real-targets-x509-replay input manifest:
    python3 scripts/real_targets.py replay --pair x509 --input {{quote(input)}} --build-manifest {{quote(manifest)}}

real-targets-check: real-json-verify
    python3 scripts/real_targets.py fetch --pair all
    python3 scripts/real_targets.py build --pair all --profile asan > run/real-targets/asan-manifests.json
    python3 scripts/real_targets.py verify --pair all --profile asan --manifest-map run/real-targets/asan-manifests.json
    python3 scripts/real_targets.py build --pair all --profile release > run/real-targets/release-manifests.json
    python3 scripts/real_targets.py verify --pair all --profile release --manifest-map run/real-targets/release-manifests.json
    python3 -m unittest tests.test_real_target_sources tests.test_real_target_semantic tests.test_real_target_png tests.test_real_target_bmp tests.test_real_target_x509 tests.test_real_target_campaigns -v

contest-paper:
    mkdir -p run/contest-paper
    latexmk -xelatex -interaction=nonstopmode -halt-on-error -file-line-error -outdir=run/contest-paper docs/contest/submission-paper.tex

contest-paper-clean:
    rm -rf run/contest-paper

check: contest-build real-json-build
    cargo check --all-targets --workspace --locked
    cargo test --workspace --locked
    python3 -m unittest discover -s tests -v
    python3 scripts/contest_demo.py verify
    python3 scripts/real_json_demo.py verify
