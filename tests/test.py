import requests
import json
import uuid
from datetime import datetime

BASE_URL = "http://localhost:5050/api"

def generate_name(prefix=""):
    """Generate unique name using timestamp and UUID"""
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    unique_id = str(uuid.uuid4())[:8]
    return f"{prefix}_{timestamp}_{unique_id}"

def test_collections():
    collection_name = generate_name("test_collection")
    print(f"Testing with collection: {collection_name}")

    # Create collection
    resp = requests.post(f"{BASE_URL}/{collection_name}")
    assert resp.status_code == 201, "Collection creation should return 201"

    # Try to create same collection again
    resp = requests.post(f"{BASE_URL}/{collection_name}")
    assert resp.status_code == 409, "Duplicate collection should return 409"

    # Check collection exists
    resp = requests.head(f"{BASE_URL}/{collection_name}")
    assert resp.status_code == 200, "Collection should exist"

    # Check non-existent collection
    resp = requests.head(f"{BASE_URL}/nonexistent_{uuid.uuid4()}")
    assert resp.status_code == 404, "Non-existent collection should return 404"

    return collection_name

def test_keys(collection_name):
    key_name = generate_name("test_key")
    print(f"Testing with key: {key_name}")
    test_value = {"test": "value"}

    # Insert key
    resp = requests.post(f"{BASE_URL}/{collection_name}/{key_name}", json=test_value)
    assert resp.status_code == 200, "Key insertion should return 200"

    # Get key
    resp = requests.get(f"{BASE_URL}/{collection_name}/{key_name}")
    assert resp.status_code == 200, "Key retrieval should return 200"
    assert resp.json() == test_value, "Retrieved value should match inserted value"

    # Check key exists
    resp = requests.head(f"{BASE_URL}/{collection_name}/{key_name}")
    assert resp.status_code == 200, "Key should exist"

    # Check non-existent key
    resp = requests.head(f"{BASE_URL}/{collection_name}/nonexistent_{uuid.uuid4()}")
    assert resp.status_code == 404, "Non-existent key should return 404"

    # Delete key
    resp = requests.delete(f"{BASE_URL}/{collection_name}/{key_name}")
    assert resp.status_code == 200, "Key deletion should return 200"

    # Try to delete again
    resp = requests.delete(f"{BASE_URL}/{collection_name}/{key_name}")
    assert resp.status_code == 404, "Deleting non-existent key should return 404"

def test_list(collection_name):
    # Insert multiple items with predictable ordering
    keys = []
    for i in range(3):
        key = f"key_{i:02d}_{uuid.uuid4()}"
        keys.append(key)
        resp = requests.post(
            f"{BASE_URL}/{collection_name}/{key}",
            json={"value": i}
        )
        assert resp.status_code == 200, f"Failed to insert test item {i}"

    # List all items
    resp = requests.get(f"{BASE_URL}/{collection_name}")
    assert resp.status_code == 200, "List should return 200"
    items = resp.json()
    assert len(items) == 3, "Should have 3 items"

    # Test range query
    resp = requests.get(f"{BASE_URL}/{collection_name}?from=0&to=1")
    assert resp.status_code == 200, "Range query should return 200"
    items = resp.json()
    assert len(items) == 2, "Range query should return 2 items"

def test_error_cases(collection_name):
    # Test invalid JSON
    resp = requests.post(f"{BASE_URL}/{collection_name}/bad_key", data="invalid json")
    assert resp.status_code == 400, "Invalid JSON should return 400"

    # Try to access key in non-existent collection
    nonexistent_collection = f"nonexistent_{uuid.uuid4()}"
    resp = requests.get(f"{BASE_URL}/{nonexistent_collection}/some_key")
    assert resp.status_code == 404, "Key in non-existent collection should return 404"

def cleanup(collection_name):
    try:
        requests.delete(f"{BASE_URL}/{collection_name}")
        print(f"Cleaned up collection: {collection_name}")
    except:
        print(f"Failed to clean up collection: {collection_name}")

if __name__ == "__main__":
    try:
        print("\nTesting collections...")
        collection_name = test_collections()

        print("\nTesting keys...")
        test_keys(collection_name)

        print("\nTesting list operations...")
        test_list(collection_name)

        print("\nTesting error cases...")
        test_error_cases(collection_name)

        print("\nAll tests passed!")

    finally:
        # Cleanup
        if 'collection_name' in locals():
            cleanup(collection_name)
