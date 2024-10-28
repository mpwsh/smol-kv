import requests
import json
import uuid
from datetime import datetime
from typing import Any
import time

BASE_URL = "http://localhost:5050/api"


class TestStats:
    def __init__(self):
        self.start_time = time.time()
        self.operation_count = 0
        self.test_count = 0

    def record_operation(self):
        self.operation_count += 1

    def record_test(self):
        self.test_count += 1

    def get_execution_time(self):
        return time.time() - self.start_time


def log_operation(
    stats: TestStats, operation: str, resource: str, status_code: int, data: Any = None
):
    """Log an API operation with optional response data"""
    stats.record_operation()
    print(f"\n➡️  {operation}: {resource}")
    print(f"📊 Status: {status_code}")
    if data:
        print("📄 Response:")
        print(json.dumps(data, indent=2))


def generate_name(prefix=""):
    """Generate unique name using timestamp and UUID"""
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    unique_id = str(uuid.uuid4())[:8]
    return f"{prefix}_{timestamp}_{unique_id}"


def test_collections(stats: TestStats):
    collection_name = generate_name("test_collection")
    print(f"\n🔵 Testing collections with: {collection_name}")
    stats.record_test()

    # Create collection
    resp = requests.post(f"{BASE_URL}/{collection_name}")
    log_operation(stats, "Create collection", collection_name, resp.status_code)
    assert resp.status_code == 201, "Collection creation should return 201"

    # Try to create same collection again
    resp = requests.post(f"{BASE_URL}/{collection_name}")
    log_operation(
        stats, "Create duplicate collection", collection_name, resp.status_code
    )
    assert resp.status_code == 409, "Duplicate collection should return 409"

    # Check collection exists
    resp = requests.head(f"{BASE_URL}/{collection_name}")
    log_operation(stats, "Check collection exists", collection_name, resp.status_code)
    assert resp.status_code == 200, "Collection should exist"

    # Check non-existent collection
    nonexistent = f"nonexistent_{uuid.uuid4()}"
    resp = requests.head(f"{BASE_URL}/{nonexistent}")
    log_operation(stats, "Check non-existent collection", nonexistent, resp.status_code)
    assert resp.status_code == 404, "Non-existent collection should return 404"

    return collection_name


def test_keys(stats: TestStats, collection_name):
    key_name = generate_name("test_key")
    print(f"\n🔵 Testing keys with: {key_name}")
    stats.record_test()
    test_value = {"test": "value", "timestamp": datetime.now().isoformat()}

    # Insert key
    resp = requests.post(f"{BASE_URL}/{collection_name}/{key_name}", json=test_value)
    log_operation(
        stats,
        "Insert key",
        f"{collection_name}/{key_name}",
        resp.status_code,
        test_value,
    )
    assert resp.status_code == 200, "Key insertion should return 200"

    # Get key
    resp = requests.get(f"{BASE_URL}/{collection_name}/{key_name}")
    log_operation(
        stats, "Get key", f"{collection_name}/{key_name}", resp.status_code, resp.json()
    )
    assert resp.status_code == 200, "Key retrieval should return 200"
    assert resp.json() == test_value, "Retrieved value should match inserted value"

    # Check key exists
    resp = requests.head(f"{BASE_URL}/{collection_name}/{key_name}")
    log_operation(
        stats, "Check key exists", f"{collection_name}/{key_name}", resp.status_code
    )
    assert resp.status_code == 200, "Key should exist"

    # Delete key
    resp = requests.delete(f"{BASE_URL}/{collection_name}/{key_name}")
    log_operation(
        stats, "Delete key", f"{collection_name}/{key_name}", resp.status_code
    )
    assert resp.status_code == 200, "Key deletion should return 200"

    # Try to delete again
    resp = requests.delete(f"{BASE_URL}/{collection_name}/{key_name}")
    log_operation(
        stats,
        "Delete non-existent key",
        f"{collection_name}/{key_name}",
        resp.status_code,
    )
    assert resp.status_code == 404, "Deleting non-existent key should return 404"


def test_list(stats: TestStats, collection_name):
    print(f"\n🔵 Testing list operations in collection: {collection_name}")
    stats.record_test()
    # Insert multiple items with predictable ordering
    keys = []
    for i in range(3):
        key = f"key_{i:02d}_{uuid.uuid4()}"
        keys.append(key)
        value = {"value": i, "timestamp": datetime.now().isoformat()}
        resp = requests.post(f"{BASE_URL}/{collection_name}/{key}", json=value)
        log_operation(
            stats,
            "Insert test item",
            f"{collection_name}/{key}",
            resp.status_code,
            value,
        )
        assert resp.status_code == 200, f"Failed to insert test item {i}"

    # List all items
    resp = requests.get(f"{BASE_URL}/{collection_name}")
    log_operation(
        stats, "List all items", collection_name, resp.status_code, resp.json()
    )
    assert resp.status_code == 200, "List should return 200"
    items = resp.json()
    assert len(items) == 3, "Should have 3 items"

    # Test range query
    resp = requests.get(f"{BASE_URL}/{collection_name}?from=0&to=1")
    log_operation(
        stats, "Range query (0-1)", collection_name, resp.status_code, resp.json()
    )
    assert resp.status_code == 200, "Range query should return 200"
    items = resp.json()
    assert len(items) == 2, "Range query should return 2 items"


def test_error_cases(stats: TestStats, collection_name):
    print(f"\n🔵 Testing error cases using collection: {collection_name}")
    stats.record_test()

    # Test invalid JSON
    resp = requests.post(f"{BASE_URL}/{collection_name}/bad_key", data="invalid json")
    log_operation(
        stats, "Post invalid JSON", f"{collection_name}/bad_key", resp.status_code
    )
    assert resp.status_code == 400, "Invalid JSON should return 400"

    # Try to access key in non-existent collection
    nonexistent_collection = f"nonexistent_{uuid.uuid4()}"
    resp = requests.get(f"{BASE_URL}/{nonexistent_collection}/some_key")
    log_operation(
        stats,
        "Get key from non-existent collection",
        f"{nonexistent_collection}/some_key",
        resp.status_code,
    )
    assert resp.status_code == 404, "Key in non-existent collection should return 404"


def cleanup(stats: TestStats, collection_name):
    try:
        resp = requests.delete(f"{BASE_URL}/{collection_name}")
        log_operation(stats, "Cleanup collection", collection_name, resp.status_code)
    except Exception as e:
        print(f"❌ Failed to clean up collection {collection_name}: {str(e)}")


if __name__ == "__main__":
    collection_name = None
    stats = TestStats()

    try:
        print("\n🚀 Starting API tests...")
        collection_name = test_collections(stats)

        print("\n🔑 Testing key operations...")
        test_keys(stats, collection_name)

        print("\n📋 Testing list operations...")
        test_list(stats, collection_name)

        print("\n⚠️  Testing error cases...")
        test_error_cases(stats, collection_name)

        exec_time = stats.get_execution_time()
        exec_time_ms = exec_time * 1000  # Convert to milliseconds
        print(f"\n✅ All tests passed!")
        print(f"📊 Test Summary:")
        print(f"   • Execution time: {exec_time_ms:.2f}ms")
        print(f"   • Test groups ran: {stats.test_count}")
        print(f"   • Total operations: {stats.operation_count}")
        print(f"   • Operations per second: {stats.operation_count / exec_time:.2f}")

    finally:
        if collection_name:
            print("\n🧹 Cleaning up...")
            cleanup(stats, collection_name)
