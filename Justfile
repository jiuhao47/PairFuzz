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

real-targets-run pair="png" seconds="30" cpu="0" profile="release" seed="138977575630630":
    python3 scripts/real_targets.py run --pair {{quote(pair)}} --profile {{quote(profile)}} --seconds {{quote(seconds)}} --cpu {{quote(cpu)}} --seed {{quote(seed)}}

real-targets-replay pair input manifest:
    python3 scripts/real_targets.py replay --pair {{quote(pair)}} --input {{quote(input)}} --build-manifest {{quote(manifest)}}

check: contest-build real-json-build
    cargo check --all-targets --workspace --locked
    cargo test --workspace --locked
    python3 -m unittest discover -s tests -v
    python3 scripts/contest_demo.py verify
    python3 scripts/real_json_demo.py verify
