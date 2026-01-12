# diffray interface harness (AFL++)

This directory contains a differential harness that:
- Dynamically loads two shared libraries (A/B) via `dlopen`.
- Executes both implementations on the same input.
- Captures separate AFL++ coverage maps for A and B (mapA/mapB) by clearing and snapshotting the AFL++ trace map between executions.
- Derives a final feedback map according to the selected strategy (7 modes).

## Build

```bash
./compile.sh
```

The resulting executable will be installed to `app/bins/.bin/diffray/diffray_program`.

## Run (example)

```bash
./diffray_program \
  --mode=single-cov \
  --prog-a=/path/to/libA.so --func-a=parseA \
  --prog-b=/path/to/libB.so --func-b=parseB
```

## Modes

- `single-cov`
- `combo-cov`
- `single-pl`
- `combo-pl`
- `single-rv`
- `combo-rv`
- `single-rv-cov`
