## Description
Minimal working setup of Actix-web with RocksDB being used as a simple JSON KV store or cache.

## Build
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
