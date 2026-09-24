'use strict'

// A real 1.21.11 protocol client for dev/via-test.sh. Packet names and shapes
// come from minecraft-protocol's versioned schemas; this test deliberately
// contains no numeric packet IDs that could make a broken translation look
// healthy after Mojang changes the protocol.

const mc = require('minecraft-protocol')
const minecraftData = require('minecraft-data')

const VERSION = '1.21.11'
const host = process.argv[2] || '127.0.0.1'
const port = parseInteger(process.argv[3], 'port', 1, 65535)
const timeoutMs = parseInteger(process.argv[4] || '60000', 'timeout', 1000, 300000)
const stableAfterCommandMs = 3000

const registry = minecraftData(VERSION)
if (!registry || registry.version.minecraftVersion !== VERSION) {
  throw new Error(`installed minecraft-data does not contain ${VERSION}`)
}

const observations = {
  play: false,
  chunk: false,
  keepAlive: false,
  commandSent: false,
  stableAfterCommand: false
}

let finished = false
let lastDisconnect = null
let stabilityTimer = null

console.log(`connecting with Minecraft ${VERSION} (protocol ${registry.version.version})`)

const client = mc.createClient({
  host,
  port,
  username: 'FotonViaE2E',
  version: VERSION,
  auth: 'offline',
  keepAlive: true,
  checkTimeoutInterval: timeoutMs
})

const deadline = setTimeout(() => {
  fail(`timed out; missing: ${missingObservations().join(', ') || 'connection stability'}`)
}, timeoutMs)

client.once('playerJoin', () => {
  observations.play = true
  console.log('entered PLAY')
  maybeSendCommand()
})

client.on('packet', (_packet, metadata) => {
  if (metadata.state !== 'play') return

  if (metadata.name === 'map_chunk' && !observations.chunk) {
    observations.chunk = true
    console.log('received a translated chunk')
    maybeSendCommand()
  }

  if (metadata.name === 'keep_alive' && !observations.keepAlive) {
    // minecraft-protocol's keepAlive client plugin writes the version-correct
    // acknowledgement synchronously from the named event emitted immediately
    // after this generic packet event.
    observations.keepAlive = true
    setImmediate(maybePass)
  }
})

client.on('kick_disconnect', packet => {
  lastDisconnect = describe(packet)
})

client.on('disconnect', packet => {
  lastDisconnect = describe(packet)
})

client.on('error', error => {
  fail(`protocol client error: ${error.stack || error.message || error}`)
})

client.on('end', reason => {
  if (!finished) {
    const suffix = lastDisconnect ? `; server disconnect: ${lastDisconnect}` : ''
    fail(`connection ended before acceptance (${reason || 'no reason'})${suffix}`)
  }
})

function maybeSendCommand () {
  if (!observations.play || !observations.chunk || observations.commandSent || finished) return

  // Unsigned commands are valid on this deliberately offline, non-secure-chat
  // test server. Using the schema name keeps this independent of packet IDs.
  client.write('chat_command', { command: 'list' })
  observations.commandSent = true
  console.log('sent /list through the translated serverbound pipeline')

  stabilityTimer = setTimeout(() => {
    observations.stableAfterCommand = true
    maybePass()
  }, stableAfterCommandMs)
}

function maybePass () {
  if (finished || missingObservations().length !== 0) return
  finished = true
  clearTimeout(deadline)
  if (stabilityTimer) clearTimeout(stabilityTimer)
  console.log('VIA CLIENT ACCEPTED: PLAY, chunk, keep-alive, command, stable connection')
  client.end('via-e2e-complete')
}

function missingObservations () {
  return Object.entries(observations)
    .filter(([, seen]) => !seen)
    .map(([name]) => name)
}

function fail (message) {
  if (finished) return
  finished = true
  clearTimeout(deadline)
  if (stabilityTimer) clearTimeout(stabilityTimer)
  console.error(`VIA CLIENT REJECTED: ${message}`)
  client.end('via-e2e-failed')
  process.exitCode = 1
}

function parseInteger (value, label, minimum, maximum) {
  const parsed = Number(value)
  if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new Error(`${label} must be an integer from ${minimum} to ${maximum}`)
  }
  return parsed
}

function describe (value) {
  try {
    return JSON.stringify(value, (_key, item) => typeof item === 'bigint' ? item.toString() : item)
  } catch (_) {
    return String(value)
  }
}
