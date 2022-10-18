## Description
Minimal working setup of Actix-web with RocksDB being used as a simple JSON KV store or cache.

## Quick Start
```bash
❯ docker run -p 5050:5050 -e DATABASE_PATH=/rocksdb \
  -v $(pwd)/rocksdb:/rocksdb \
  ghcr.io/mpwsh/smol-kv:amd64-latest #arm64-latest image also available
```


## Build from source
`clang` is required to build.  Install with `pacman` or `apt`. Check the [Dockerfile](Dockerfile) for guidance.

```bash
❯ cargo build --release
```

## Configuration
Set the following env vars to configure the server (optional)
```bash
PORT=5050
WORKERS=4
LOG_LEVEL=info
DATABASE_PATH=./rocksdb
```

At this point you can run the binary and the server should start.

## Usage
### Get value
```bash
❯ curl -i -X GET -H "Content-Type: application/json" http://localhost:5050/api/yourkey
# Returns error 404 if key was not found.
```

### Create new key with value
Value needs to be in valid UTF-8 and in JSON format, parsing will fail otherwise.
```bash
❯ curl -X POST -H "Content-Type: application/json" -d '{"name":"test"}' http://localhost:5050/api/yourkey
# output
{"name":"test"}
```

Hitting `http://localhost:5050/api/yourkey` with a `GET` request should output the value now instead of responding with 404.
```bash
❯ curl http://localhost:5050/api/yourkey
{"name":"test"}
```


### Trying invalid json
```bash
❯ curl -X POST -H "Content-Type: application/json" -d 'invalid' http://localhost:5050/api/wontwork
# output
{"msg":"Parsing failed. value is not in JSON Format","status":400}
# No data was saved
```


### Delete a key
```bash
❯ curl -i -X DELETE -H "Content-Type: application/json" http://localhost:5050/api/yourkey
# No output
# 200 OK means operation succeeded
# Responds with error 500 if something went wrong.
```


## Benchmarks
A [Drill](https://github.com/fcsonline/drill) plan is available in the [benchmark](benchmark) folder.
To run install `drill` using `cargo` and execute:
```bash
drill --benchmark benchmark/plan.yaml  --stats
```


### Results
'Failed requests' are 404 assertions, so those are actually successful.

> System details: AMD Ryzen 7 3700X, 32 GB Ram, Samsung SSD 970 EVO Plus

```yaml
iterations: 2000
concurrency: 4
rampup: 4
```

```text
Time taken for tests      4.2 seconds
Total requests            8000
Successful requests       6000
Failed requests           2000
Requests per second       1886.13 [#/sec]
Median time per request   0ms
Average time per request  1ms
Sample standard deviation 1ms
99.0'th percentile        5ms
99.5'th percentile        6ms
99.9'th percentile        6ms
```
