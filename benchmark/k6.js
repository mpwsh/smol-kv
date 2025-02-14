import http from "k6/http";
import { sleep } from "k6";
import { randomIntBetween, randomItem } from "https://jslib.k6.io/k6-utils/1.2.0/index.js";

export let options = {
  scenarios: {
    game_simulation: {
      executor: 'constant-vus',
      vus: 50,
      duration: '1m', // Increased duration
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

export default function () {
  const playerId = parseInt(__VU);
  const matchId = "bench";
  const headers = { 
    "X-SECRET-KEY": "testing"
  };

  // Setup collection only ONCE per VU
  if (__ITER === 0) {
    const createResp = http.put(
      `http://127.0.0.1:5050/api/${matchId}`,
      null,
      { headers }
    );
  }


  const startTime = new Date().getTime();

  // write player data - check response
  const playerData = generateRandomPlayerData(playerId);
  const writeResp = http.put(
    `http://127.0.0.1:5050/api/${matchId}/${playerId}`,
    JSON.stringify(playerData),
    { headers }
  );
    if (writeResp.status !== 201) {
      console.error(`Failed to write player data: ${writeResp.status}`);
    }


  // query - check response
  const randomQuery = randomItem(queryTypes);
  const queryBody = {
    keys: true,
    limit: 10,
    query: randomQuery.query
  };

  const queryResp = http.post(
    `http://127.0.0.1:5050/api/${matchId}`,
    JSON.stringify(queryBody),
    {headers},
  );
    if (queryResp.status !== 200) {
      console.error(`Failed to query: ${queryResp.status}`);
    }


  const endTime = new Date().getTime();
  const executionTime = endTime - startTime;
   
  const sleepTime = Math.max(0, 7.8125 - executionTime);
  sleep(sleepTime / 1000);
}
