import http from "k6/http";
import { sleep } from "k6";
import { randomIntBetween, randomItem } from "https://jslib.k6.io/k6-utils/1.2.0/index.js";

export let options = {
  scenarios: {
    players: {
      executor: 'constant-vus',
      vus: 50,
      duration: '30s',
    },
  },
};

const playerStates = [
  "Idle",
  "Moving",
  "Dashing",
  "PreparingPrimary",
  "CastingPrimary",
  "PreparingSecondary",
  "CastingSecondary",
  "Resting",
  "Meditating",
  "Dead",
];

function generateRandomPlayerBundle(playerId) {
  return {
    player: playerId,
    name: `Player${playerId}`,
    health: Math.random() * 100,
    mana: Math.random() * 250,
    position: Math.random() * 100,
    size: Math.random() * 10,
    state: randomItem(playerStates),
    angle: Math.random() * 360,
    color: Math.random(),
    winner: Math.random() < 0.5,
    rounds_won: randomIntBetween(0, 10),
    points: randomIntBetween(-100, 100),
  };
}

export default function () {
  const playerId = parseInt(__VU);
  const params = { headers: { "Content-Type": "application/json" } };
  let matchId = "test-match"

  //Create collection
  http.post(
      `http://127.0.0.1:5050/api/${matchId}`,
      params
    );
  while (true) {
    const startTime = new Date().getTime();
    
    const playerBundle = generateRandomPlayerBundle(playerId);

    // Save Player data 
    http.post(
      `http://127.0.0.1:5050/api/${matchId}/${playerId}`,
      JSON.stringify(playerBundle),
      params
    );
    
    const endTime = new Date().getTime();
    const executionTime = endTime - startTime;
    
    // Calculate sleep time to maintain 128 Hz (7.8125 ms per tick) plus 10 ms delay
     const sleepTime = Math.max(0, 7.8125 - executionTime);
    sleep(sleepTime / 1000);  // sleep function takes seconds, not milliseconds
  }
}
