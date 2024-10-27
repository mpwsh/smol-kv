import http from "k6/http";
import { sleep } from "k6";
import { randomIntBetween, randomItem } from "https://jslib.k6.io/k6-utils/1.2.0/index.js";
import { SharedArray } from "k6/data";

export let options = {
  scenarios: {
    players: {
      executor: 'constant-vus',
      vus: 50,  // Number of VUs
      duration: '30s',
    },
  },
};

const playerStates = [
  "Idle", "Moving", "Dashing", "PreparingPrimary", 
  "CastingPrimary", "PreparingSecondary", "CastingSecondary", 
  "Resting", "Meditating", "Dead"
];

// Shared array to store the initial state of all players
const initialPlayers = new SharedArray("players", function () {
  return Array.from({ length: 50 }, (_, i) => ({
    player: i + 1,
    name: `Player${i + 1}`,
    health: 100,  // Initial values
    mana: 250,
    position: 0,
    size: 5,
    state: "Idle",
    angle: 0,
    color: 0.5,
    winner: false,
    rounds_won: 0,
    points: 0,
  }));
});

// Function to clone a player object
function clonePlayer(player) {
  return Object.assign({}, player);
}

// Function to update a player's data for each tick
function updatePlayerData(player) {
  return Object.assign({}, player, {
    health: parseFloat((Math.random() * 100).toFixed(2)),
    mana: parseFloat((Math.random() * 250).toFixed(2)),
    position: parseFloat((Math.random() * 100).toFixed(2)),
    size: parseFloat((Math.random() * 10).toFixed(2)),
    state: randomItem(playerStates),
    angle: parseFloat((Math.random() * 360).toFixed(2)),
    color: parseFloat(Math.random().toFixed(2)),
    winner: Math.random() < 0.5,
    rounds_won: randomIntBetween(0, 10),
    points: randomIntBetween(-100, 100),
  });
}


export default function () {
  const params = { headers: { "Content-Type": "application/json" } };
  let matchId = "test-match";
  let ipAddress = "192.168.1.51";
  let port = "5050";
  let tick = 0;

  // Create collection
  http.post(`http://${ipAddress}:${port}/api/${matchId}`, params);

  while (true) {
    const startTime = new Date().getTime();

    // Clone and update all players' data for the current tick
    const updatedPlayers = initialPlayers.map(clonePlayer).map(updatePlayerData);

    // Send the array of all players' data in a single request
    const payload = JSON.stringify({
      tick: tick,
      players: updatedPlayers,
    });

    http.post(
      `http://${ipAddress}:${port}/api/${matchId}/tick${tick}`,
      payload,
      params
    );

    tick++;  // Increment the tick counter

    const endTime = new Date().getTime();
    const executionTime = endTime - startTime;

    // Calculate sleep time to maintain 128 Hz (7.8125 ms per tick) plus 10 ms delay
    const sleepTime = Math.max(0, 7.8125 - executionTime);
    sleep(sleepTime / 1000);  // sleep function takes seconds, not milliseconds
  }
}

