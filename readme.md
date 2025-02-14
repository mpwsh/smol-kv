## Description

Minimal working setup of Actix-web with RocksDB being used as a simple JSON KV store or cache.

## Quick Start

```bash
❯ docker run -p 5050:5050 -e DATABASE_PATH=/rocksdb \
  -v $(pwd)/rocksdb:/rocksdb \
  mpwsh/smol-kv:latest
```

## Build from source

`clang` is required to build. Install with `pacman` or `apt`. Check the [Dockerfile](Dockerfile) for guidance.

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
ADMIN_TOKEN=yourtoken
```

At this point you can run the binary and the server should start.

## Usage

### Create a collection

```bash
❯ curl -X PUT localhost:5050/api/mycollection
```

### Get value

```bash
❯ curl -i -X GET -H "Content-Type: application/json" http://localhost:5050/api/mycollection/yourkey
# Returns error 404 if key was not found.
```

### Create new key with value

Value needs to be in valid UTF-8 and in JSON format, parsing will fail otherwise.

```bash
❯ curl -X PUT -H "Content-Type: application/json" -d '{"name":"test"}' http://localhost:5050/api/mycollection/yourkey
# output
{"name":"test"}
```

Hitting `http://localhost:5050/api/yourkey` with a `GET` request should output the value now instead of responding with 404.

```bash
❯ curl http://localhost:5050/api/mycollection/yourkey
{"name":"test"}
```

### Trying invalid json

```bash
❯ curl -X PUT -H "Content-Type: application/json" -d 'invalid' http://localhost:5050/api/mycollection/wontwork
# output
Parsing failed. value is not in JSON Format
# No data was saved
```

### Delete a key

```bash
❯ curl -i -X DELETE -H "Content-Type: application/json" http://localhost:5050/api/mycollection/yourkey
# No output
# 200 OK means operation succeeded
# Responds with error 500 if something went wrong.
```

## Benchmark

A [Drill](https://github.com/fcsonline/drill) plan is available in the [benchmark](benchmark) folder.
To run install `drill` using `cargo` and execute:

```bash
drill --benchmark benchmark/plan.yaml  --stats
```

### Plan

```yaml
iterations: 2000
concurrency: 4
rampup: 4
```

> System details: AMD Ryzen 7 3700X, 32 GB Ram, Samsung SSD 970 EVO Plus

### Results

'Failed requests' are 404 assertions, so those are actually successful.

```text
Time taken for tests      3.6 seconds
Total requests            8000
Successful requests       6000
Failed requests           2000
Requests per second       2239.79 [#/sec]
Median time per request   0ms
Average time per request  2ms
Sample standard deviation 3ms
99.0'th percentile        11ms
99.5'th percentile        11ms
99.9'th percentile        11ms
```

Or using `apache-bench`

```bash
apt update && apt install -y apache-bench
ab -n 20000 -c 8 -H 'Authorization: supersecret' -p ./benchmark/data.json -T 'application/json' -rk http://127.0.0.1:5050/benchmark
```

### Results

```
Requests per second:    41251.57 [#/sec] (mean)
Time per request:       0.194 [ms] (mean)
Time per request:       0.024 [ms] (mean, across all concurrent requests)
Transfer rate:          11843.71 [Kbytes/sec] received
                        12488.27 kb/s sent
                        24331.98 kb/s total
```
