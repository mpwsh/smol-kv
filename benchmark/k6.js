import http from "k6/http";
import { sleep } from "k6";
import { randomIntBetween, randomItem } from "https://jslib.k6.io/k6-utils/1.2.0/index.js";

// Configuration
const BASE_URL = "http://localhost:5050"
const MATCH_ID = "bench";
const HEADERS = { 
  "X-SECRET-KEY": "testing"
};

export let options = {
  scenarios: {
    game_simulation: {
      executor: 'constant-vus',
      vus: 50,
      duration: '1m',
    },
  },
};

const playerStates = [
  "Idle", "Moving", "Fighting", "Dead", "Respawning",
  "Trading", "Crafting", "Mining", "Fishing", "Healing"
];

const queryTypes = [
  { query: { "==": [{ "var": "state" }, "Dead"] } },
  { query: { ">=": [{ "var": "health" }, 50] } },
  { query: { "<": [{ "var": "points" }, 0] } },
  { query: { "and": [
    { ">": [{ "var": "health" }, 20] },
    { "<": [{ "var": "mana" }, 100] }
  ]}},
  { query: { "==": [{ "var": "winner" }, true] } }
];

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
  const createResp = http.put(
    `${BASE_URL}/api/${MATCH_ID}`,
    null,
    { headers: HEADERS }
  );
  
  if (createResp.status !== 201) {
    console.error(`Failed to create collection: ${createResp.status}`);
    // We don't throw an error here because we want the benchmark to continue
  }
}

// Default function contains the core benchmark logic
export default function () {
    const BATCH_SIZE = 50;  // assuming 50 VUs, we'll send all at once
    const startTime = new Date().getTime();
    
    // if we're VU #1, we'll handle the batch write for everyone
    if (__VU === 1) {
        const batchData = [];
        
        // generate data for all players
        for (let i = 1; i <= BATCH_SIZE; i++) {
            batchData.push({
                key: i.toString(),
                value: generateRandomPlayerData(i)
            });
        }

        // send the batch
        const writeResp = http.put(
            `${BASE_URL}/api/${MATCH_ID}/_batch`,
            JSON.stringify(batchData),
            { headers: HEADERS }
        );

        if (writeResp.status !== 201) {
            console.error(`Failed to write batch data: ${writeResp.status}`);
        }
    }

    // everyone still does queries
    const randomQuery = randomItem(queryTypes);
    const queryBody = {
        keys: true,
        limit: 10,
        query: randomQuery.query
    };
   
    /*
    const queryResp = http.post(
        `${BASE_URL}/api/${MATCH_ID}`,
        JSON.stringify(queryBody),
        { headers: HEADERS },
    );

    if (queryResp.status !== 200) {
        console.error(`Failed to query: ${queryResp.status}`);
    }*/

    const endTime = new Date().getTime();
    const executionTime = endTime - startTime;
    const sleepTime = Math.max(0, 7.8125 - executionTime);
    sleep(sleepTime / 1000);
}
