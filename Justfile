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

real-json-verify:
    python3 scripts/real_json_demo.py verify

real-json-replay input="examples/real-json/known-difference.json":
    python3 scripts/real_json_demo.py replay {{quote(input)}}

check:
    cargo check --all-targets --workspace --locked
    cargo test --workspace --locked
    python3 -m unittest discover -s tests -v
    python3 scripts/contest_demo.py verify
    python3 scripts/real_json_demo.py verify
