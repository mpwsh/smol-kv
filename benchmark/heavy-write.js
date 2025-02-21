import http from "k6/http";
import { randomIntBetween, randomItem } from "https://jslib.k6.io/k6-utils/1.2.0/index.js";

const BASE_URL = "http://localhost:5050";
const MATCH_ID = "bench";
const HEADERS = { 
    "X-SECRET-KEY": "testing",
    "Content-Type": "application/json"
};

const PLAYERS_PER_BATCH = 10;
const TARGET_RPS = 6000;

export let options = {
    scenarios: {
        constant_load: {
            executor: 'constant-arrival-rate',
            rate: TARGET_RPS,
            timeUnit: '1s',
            duration: '30s',
            preAllocatedVUs: 100,
            maxVUs: 250,
        },
    },
    thresholds: {
        http_req_duration: ['p(95)<10', 'p(99)<15']
    }
};
const playerStates = ["Moving", "Moving", "Moving", "Idle", "Idle", "Fighting", "Healing", "Dead"];

function generatePlayerData(playerId) {
    return {
        player: playerId,
        name: `Player${playerId}`,
        health: 100,
        mana: Math.random() * 250,
        position: {
            x: Math.random() * 1000,
            y: Math.random() * 1000,
            z: 0
        },
        state: randomItem(playerStates),
        velocity: {
            x: Math.random() * 2 - 1,
            y: Math.random() * 2 - 1,
            z: 0
        },
        winner: Math.random() < 0.5,
        points: randomIntBetween(-100, 100),
        last_action: Date.now(),
        active_buffs: randomIntBetween(0, 5),
        damage_dealt: Math.random() * 1000,
        healing_done: Math.random() * 500
    };
}

export function setup() {
    const createResp = http.put(
        `${BASE_URL}/api/${MATCH_ID}`,
        null,
        { headers: HEADERS }
    );
    if (createResp.status !== 201) {
        console.error(`Failed to create collection: ${createResp.status}`);
    }
}

export default function () {
    const batchData = [];
    for (let i = 0; i < PLAYERS_PER_BATCH; i++) {
        batchData.push({
            key: `${__VU}-${__ITER}-${i}`,
            value: generatePlayerData(i)
        });
    }

    const writeResp = http.put(
        `${BASE_URL}/api/${MATCH_ID}/_batch`,
        JSON.stringify(batchData),
        { headers: HEADERS }
    );

    if (writeResp.status !== 201) {
        console.error(`Failed to save batch: ${writeResp.status}`);
    }
}
