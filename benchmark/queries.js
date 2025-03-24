import http from "k6/http";
import { sleep } from "k6";
import { randomIntBetween, randomItem } from "https://jslib.k6.io/k6-utils/1.2.0/index.js";
import { check } from "k6";

// Configuration
const BASE_URL = "http://localhost:5050";
const MATCH_ID = "bench";
const HEADERS = { 
  "X-SECRET-KEY": "testing",
  "Content-Type": "application/json"
};

export let options = {
  scenarios: {
    game_simulation: {
      executor: 'constant-vus',
      vus: 200,
      duration: '30s',
    },
  },
};

// Player state options
const playerStates = [
  "Idle", "Moving", "Fighting", "Dead", "Respawning",
  "Trading", "Crafting", "Mining", "Fishing", "Healing"
];

// Updated queries using JSONPath format
const queryTypes = [
  { name: "Find dead players", query: "$[?@.state=='Dead']" },
  { name: "Find healthy players", query: "$[?@.health>=50]" },
  { name: "Find players with negative points", query: "$[?@.points<0]" },
  { name: "Find players with good health but low mana", query: "$[?@.health>20&&@.mana<100]" },
  { name: "Find winners", query: "$[?@.winner==true]" },
  { name: "Find players in specific positions", query: "$[?@.position.x>500&&@.position.y<300]" },
  { name: "Find players with recent actions", query: "$[?@.last_action>1000000000000]" },
  { name: "Find players with buffs", query: "$[?@.active_buffs>0]" },
  { name: "Find players with high damage", query: "$[?@.damage_dealt>500]" },
];

// Function to generate random player data
function generateRandomPlayerData(playerId) {
  return {
    player: playerId,
    name: `Player${playerId}`,
    health: Math.random() * 100,
    mana: Math.random() * 250,
    position: {
      x: Math.random() * 1000,
      y: Math.random() * 1000,
      z: Math.random() * 100
    },
    state: randomItem(playerStates),
    velocity: {
      x: Math.random() * 20 - 10,
      y: Math.random() * 20 - 10,
      z: Math.random() * 10 - 5
    },
    winner: Math.random() < 0.5,
    points: randomIntBetween(-100, 100),
    last_action: Date.now(),
    active_buffs: randomIntBetween(0, 5),
    damage_dealt: Math.random() * 1000,
    healing_done: Math.random() * 500
  };
}

// Setup function runs once before the test starts
export function setup() {
  console.log("Creating test collection...");
  const createResp = http.put(
    `${BASE_URL}/api/${MATCH_ID}`,
    null,
    { headers: HEADERS }
  );
  
  check(createResp, {
    "Collection created successfully": (r) => r.status === 201 || r.status === 200
  });
  
  if (createResp.status !== 201 && createResp.status !== 200) {
    console.error(`Failed to create collection: ${createResp.status} - ${createResp.body}`);
  }

  return { startTime: new Date().getTime() };
}

// Teardown function runs once after the test completes
export function teardown(data) {
  const duration = (new Date().getTime() - data.startTime) / 1000;
  console.log(`Test completed in ${duration} seconds`);
  
  // Optionally delete the test collection
  // const deleteResp = http.del(`${BASE_URL}/api/${MATCH_ID}`, null, { headers: HEADERS });
  // console.log(`Collection deletion response: ${deleteResp.status}`);
}

// Default function contains the core benchmark logic
export default function () {
  const BATCH_SIZE = 50;  // assuming 50 VUs, we'll send all at once
  const startTime = new Date().getTime();
  
  // VU #1 handles batch writes for everyone
  if (__VU === 1) {
    const batchData = [];
    
    // Generate data for all players
    for (let i = 1; i <= BATCH_SIZE; i++) {
      batchData.push({
        key: i.toString(),
        value: generateRandomPlayerData(i)
      });
    }
    
    // Send the batch
    const writeResp = http.put(
      `${BASE_URL}/api/${MATCH_ID}/_batch`,
      JSON.stringify(batchData),
      { headers: HEADERS }
    );
    
    check(writeResp, {
      "Batch write successful": (r) => r.status === 201 || r.status === 200
    });
    
    if (writeResp.status !== 201 && writeResp.status !== 200) {
      console.error(`Failed to write batch data: ${writeResp.status} - ${writeResp.body}`);
    }
  }
  
  // All VUs perform queries
  const randomQueryType = randomItem(queryTypes);
  
  // Try different query methods for more thorough testing
  const queryMethod = randomIntBetween(1, 3);
  
  let queryResp;
  
  switch (queryMethod) {
    case 1:
      // Method 1: POST to /query endpoint with JSONPath in body
      const queryBody = {
        query: randomQueryType.query,
        keys: Math.random() > 0.5, // Randomly test with or without keys
        limit: randomIntBetween(5, 20)
      };
      
      queryResp = http.post(
        `${BASE_URL}/api/${MATCH_ID}`,
        JSON.stringify(queryBody),
        { headers: HEADERS }
      );
      break;
      
    case 2:
      // Method 2: GET with query in URL param (properly encoded)
      const encodedQuery = encodeURIComponent(randomQueryType.query);
      queryResp = http.get(
        `${BASE_URL}/api/${MATCH_ID}?query=${encodedQuery}&keys=${Math.random() > 0.5}&limit=${randomIntBetween(5, 20)}`,
        { headers: HEADERS }
      );
      break;
      
    case 3:
      // Method 3: Basic range query (no JSONPath)
      queryResp = http.get(
        `${BASE_URL}/api/${MATCH_ID}?from=1&to=25&keys=${Math.random() > 0.5}&limit=${randomIntBetween(5, 20)}`,
        { headers: HEADERS }
      );
      break;
  }
  
  check(queryResp, {
    [`Query (${randomQueryType.name}) successful`]: (r) => r.status === 200
  });
  
  if (queryResp.status !== 200) {
    console.error(`Failed query: ${queryResp.status} - Query: ${randomQueryType.query}`);
  } else {
    // Optional: Parse and validate response
    try {
      const results = JSON.parse(queryResp.body);
      check(results, {
        "Results are valid": (r) => Array.isArray(r)
      });
    } catch (e) {
      console.error(`Invalid response: ${e.message}`);
    }
  }
  
  // Calculate and apply sleep to maintain consistent request rate
  const endTime = new Date().getTime();
  const executionTime = endTime - startTime;
  const sleepTime = Math.max(0, 7.8125 - executionTime);
  sleep(sleepTime / 1000);
}
