# Smol-KV

## Description

smolkv is a lightweight JSON document store built with actix-web using RocksDB as storage layer.
A [CLI](https://github.com/mpwsh/smolkv-client/blob/main/examples/cli.rs) and HTTP [client library](https://github.com/mpwsh/smolkv-client) is also available for programatic use. Access to rocksDB internals is provided by [this library](https://github.com/mpwsh/rocksdb-client).

Use at your own risk, im not responsible for any kind of dataloss caused by my sloppy code.

## Quick Start

```bash
❯ docker run -p 5050:5050 -e DATABASE_PATH=/rocksdb \
  -v $(pwd)/rocksdb:/rocksdb \
  mpwsh/smol-kv:latest
```

If you used this step jump directly to Usage.

## Build from source

`clang` is required to build. Install with `pacman` or `apt`. Check the [Dockerfile](Dockerfile) for guidance.

```bash
❯ cargo build --release
```

## Configuration

You can use the following env vars to configure the server (optional)

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
curl -sX PUT localhost:5050/api/mycollection
{"message":"Collection mycollection created","secret_key":"<generated-secret-key>"}
# You can also provide your own to avoid random secret generation
curl -sX PUT -H "X-SECRET-KEY: verysecure" localhost:5050/api/mycollection
{"message":"Collection mycollection created","secret_key":"verysecure"}
```

### Create new key with value

Value needs to be in valid UTF-8 and in JSON format, parsing will fail otherwise.

```bash
curl -sX PUT -H "X-SECRET-KEY: verysecure" -d '{"name":"test", "age": 10}' http://localhost:5050/api/mycollection/test
# output
{"name":"test", "age": 10}
```

### Retrieve a value

```bash
curl -H "X-SECRET-KEY: verysecure" http://localhost:5050/api/mycollection/test
{"name":"test", "age": 10}
```

### Batch Operations

```bash
# With Specific Key/Values
curl -X PUT -H "X-SECRET-KEY: verysecure" -H "Content-Type: application/json" \
  -d '[
    {"key": "user1", "value": {"name": "Alice", "age": 30}},
    {"key": "user2", "value": {"name": "Bob", "age": 25}},
    {"key": "user3", "value": {"name": "Charlie", "age": 35}}
  ]' \
  http://localhost:5050/api/mycollection/_batch
```

Batch import values with keys from JSON File. Using [Earth Meteorite Landings](https://data.nasa.gov/resource/y77d-th95.json) dataset.

```bash
# Create the collection 
curl -sX PUT -H "X-SECRET-KEY: verysecure" localhost:5050/api/landings
# import the values using a property from the json file as key
curl -X POST -F "file=@y77d-th95.json" -H "X-SECRET-KEY: verysecure" "http://localhost:5050/api/landings/_import?key=name"
{"message":"Successfully imported 1000 items","imported_count":1000,"collection":"mycollection","errors":null}
```

### List a collection

```bash
curl -H "X-SECRET-KEY: verysecure" http://localhost:5050/api/mycollection
[{"key":"test","value":{"age":10,"name":"test"}},{"key":"user1","value":{"age":30,"name":"Alice"}},{"key":"user2","value":{"age":25,"name":"Bob"}},{"key":"user3","value":{"age":35,"name":"Charlie"}}]

# Get values without keys
curl -H "X-SECRET-KEY: verysecure" http://localhost:5050/api/mycollection?keys=false
[{"age":10,"name":"test"},{"age":30,"name":"Alice"},{"age":25,"name":"Bob"},{"age":35,"name":"Charlie"}]
# Other available query parameters for list -- from,to,limit,order
curl -H "X-SECRET-KEY: verysecure" "http://localhost:5050/api/mycollection?keys=false&limit=1&order=desc"
[{"age":35,"name":"Charlie"}]
```

### JSONPath Queries

```bash
# Find users over 25 years old
curl -X POST -H "X-SECRET-KEY: verysecure" -H "Content-Type: application/json" \
  -d '{"query": "$[?@.age>25]"}' \
  http://localhost:5050/api/mycollection

# Combine multiple conditions
curl -X POST -H "X-SECRET-KEY: verysecure" -H "Content-Type: application/json" -d '{"query": "$[?@.age>25&&@.name==\"Alice\"]"}'   http://localhost:5050/api/mycollection
[{"key":"user1","value":{"age":30,"name":"Alice"}}]
```

### Event subscription

Open a new terminal and subscribe to a collection

```bash
curl -H "X-SECRET-KEY: verysecure" localhost:5050/api/mycollection/_subscribe
data: {"collection":"mycollection","type":"connected"}
data: {"key":"test","operation":"Create","value":{"age":10,"name":"test","serverTime":1742798981773}}
data: {"key":"test","operation":"Delete","value":null}
```

reqs sent:

```bash
curl -sX PUT -H "X-SECRET-KEY: verysecure" -d '{"name":"test", "age": 10}' http://localhost:5050/api/mycollection/test
curl -sX DELETE -H "X-SECRET-KEY: verysecure" http://localhost:5050/api/mycollection/test
```

### Database Management (Backup/Restore)

```bash
# Backup a collection
curl -X POST -H "X-SECRET-KEY: verysecure" http://localhost:5050/api/mycollection/_backup
{"message":"Backup started","id":"aRprhdOXoMrbrjyfd12bz","collection":"mycollection"}

# Check backup status
curl -sX GET -H "X-SECRET-KEY: verysecure" http://localhost:5050/api/mycollection/_backup/status?id=<backup-id>
{
  "id": "aRprhdOXoMrbrjyfd12bz",
  "collection": "mycollection",
  "started_at": "2025-03-24T06:38:03.324804Z",
  "finished_at": "2025-03-24T06:38:03.326524Z",
  "status": "completed",
  "url": "/backups/mycollection-aRprhdOXoMrbrjyfd12bz.sst",
  "error": null
}

# Download the collection backup
curl -o mybackup.sst http://localhost:5050/backups/mycollection-aRprhdOXoMrbrjyfd12bz.sst

# Upload a backup
curl -X POST -F "file=@mybackup.sst" -H "X-SECRET-KEY: verysecure" "http://localhost:5050/api/mycollection/_backup/upload"
{"message":"Backup file uploaded successfully","id":"sCzNjkv7JeGNbsPR1MRNV","collection":"mycollection"}

# Restore a collection using Backup ID
curl -X POST -H "X-SECRET-KEY: verysecure" \
  http://localhost:5050/api/mycollection/_restore?backup_id=aRprhdOXoMrbrjyfd12bz
{"message":"Restore started","id":"aRprhdOXoMrbrjyfd12bz","collection":"mycollection"}

# check the restore status
curl -H "X-SECRET-KEY: verysecure" \
http://localhost:5050/api/mycollection/_restore/status?id=aRprhdOXoMrbrjyfd12bz
{"id":"aRprhdOXoMrbrjyfd12bz","collection":"mycollection","started_at":"2025-03-24T07:18:39.653921Z","finished_at":"2025-03-24T07:18:39.657541Z","status":"completed","error":null}
```

### Delete a key

```bash
curl -i -X DELETE -H "X-SECRET-KEY: verysecure" http://localhost:5050/api/mycollection/test
HTTP/1.1 200 OK
content-length: 0
vary: Origin, Access-Control-Request-Method, Access-Control-Request-Headers
access-control-allow-credentials: true
date: Sun, 23 Mar 2025 19:49:44 GMT
```
