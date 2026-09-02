// Two Bedrock-adjacent test clients, driven from `dev/bedrock-test.sh`.
//
// `join` is a simulated Bedrock client (PrismarineJS `bedrock-protocol`,
// `offline: true`) aimed at Geyser, following the shape Stage 0 proved by
// hand (`design/bedrock-stage0-findings.md`). It exists to demonstrate the
// shared-port claim from the Bedrock side and to show Geyser's own Xbox-login
// gate is live. It does NOT reach the world in this test: Geyser's pinned
// config hardcodes `advanced.bedrock.validate-bedrock-login: true`
// (`foton-bedrock/src/geyser.rs`, `render_config`) -- correctly, for
// production -- and that check runs before any Java connection to Foton is
// attempted, entirely independent of Foton. A synthetic, non-Xbox-authenticated
// client cannot pass it, and there is no config knob to relax it for testing
// (Stage 0's Step 4 hit exactly this and said so explicitly). Real Bedrock
// players authenticate through Xbox Live in front of Geyser and never touch
// this gate.
//
// `floodgate` is not a Bedrock client at all: it is a raw Java client that
// crafts and encrypts a Floodgate handshake payload itself, using the exact
// wire format `foton-bedrock/src/floodgate.rs` decodes and the real shared
// key the server under test generated for its own Geyser
// (`<run>/bedrock/key.pem`). This is what lets the test exercise Foton's
// production Floodgate login branch -- identity derivation, the username
// prefix, and UUID persistence across a reconnect -- end to end, without an
// Xbox account, and without weakening anything: the bytes this sends are
// exactly what a real Geyser, fed a real Xbox-authenticated Bedrock player,
// would send to Foton on the Java side.
'use strict'

const net = require('net')
const fs = require('fs')
const crypto = require('crypto')
const zlib = require('zlib')

const PROTOCOL_VERSION = 776

// -- varint / string helpers, mirroring dev/join.py's wire format ----------

function writeVarint (value) {
  const out = []
  let v = value >>> 0
  do {
    let b = v & 0x7f
    v >>>= 7
    if (v !== 0) b |= 0x80
    out.push(b)
  } while (v !== 0)
  return Buffer.from(out)
}

function readVarint (buffer, offset) {
  let value = 0
  let shift = 0
  let pos = offset
  for (let i = 0; i < 5; i++) {
    if (pos >= buffer.length) return null
    const byte = buffer[pos]
    pos += 1
    value |= (byte & 0x7f) << shift
    if ((byte & 0x80) === 0) return [value >>> 0, pos]
    shift += 7
  }
  throw new Error('varint too long')
}

function writeString (text) {
  const raw = Buffer.from(text, 'utf8')
  return Buffer.concat([writeVarint(raw.length), raw])
}

function readString (buffer, offset) {
  const lengthResult = readVarint(buffer, offset)
  if (!lengthResult) return null
  const [length, afterLength] = lengthResult
  if (buffer.length < afterLength + length) return null
  return [buffer.slice(afterLength, afterLength + length).toString('utf8'), afterLength + length]
}

function formatUuid (bytes) {
  const hex = bytes.toString('hex')
  return [hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16), hex.slice(16, 20), hex.slice(20)].join('-')
}

// -- a framed Java connection, with compression once the server enables it --

class JavaConnection {
  constructor (socket) {
    this.socket = socket
    this.buffer = Buffer.alloc(0)
    this.compressionThreshold = -1
    this.queue = []
    this.waiters = []
    socket.on('data', (chunk) => {
      this.buffer = Buffer.concat([this.buffer, chunk])
      this._drain()
    })
    socket.on('close', () => this._fail(new Error('connection closed by the server')))
    socket.on('error', (error) => this._fail(error))
  }

  _fail (error) {
    while (this.waiters.length > 0) {
      const [, reject] = this.waiters.shift()
      reject(error)
    }
  }

  _drain () {
    for (;;) {
      const lengthResult = readVarint(this.buffer, 0)
      if (!lengthResult) return
      const [length, afterLength] = lengthResult
      if (this.buffer.length < afterLength + length) return
      const frame = this.buffer.slice(afterLength, afterLength + length)
      this.buffer = this.buffer.slice(afterLength + length)
      this._handleFrame(frame)
    }
  }

  _handleFrame (frame) {
    let body = frame
    if (this.compressionThreshold >= 0) {
      const [uncompressedLength, afterVarint] = readVarint(frame, 0)
      const rest = frame.slice(afterVarint)
      body = uncompressedLength > 0 ? zlib.inflateSync(rest) : rest
    }
    const idResult = readVarint(body, 0)
    if (!idResult) return
    const [packetId, afterId] = idResult
    const item = { packetId, payload: body.slice(afterId) }
    const waiter = this.waiters.shift()
    if (waiter) {
      waiter[0](item)
    } else {
      this.queue.push(item)
    }
  }

  receive (timeoutMs, what) {
    const pending = this.queue.length > 0
      ? Promise.resolve(this.queue.shift())
      : new Promise((resolve, reject) => this.waiters.push([resolve, reject]))
    return withTimeout(pending, timeoutMs, what)
  }

  send (packetId, payload) {
    const body = Buffer.concat([writeVarint(packetId), payload || Buffer.alloc(0)])
    let frame
    if (this.compressionThreshold < 0) {
      frame = body
    } else if (body.length >= this.compressionThreshold) {
      frame = Buffer.concat([writeVarint(body.length), zlib.deflateSync(body)])
    } else {
      frame = Buffer.concat([writeVarint(0), body])
    }
    this.socket.write(Buffer.concat([writeVarint(frame.length), frame]))
  }
}

function withTimeout (promise, ms, what) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timed out waiting for ${what}`)), ms)
    promise.then(
      (value) => { clearTimeout(timer); resolve(value) },
      (error) => { clearTimeout(timer); reject(error) },
    )
  })
}

function connect (port) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection({ host: '127.0.0.1', port })
    socket.once('connect', () => resolve(new JavaConnection(socket)))
    socket.once('error', reject)
  })
}

// -- Floodgate envelope, mirroring foton-bedrock/src/floodgate.rs -----------

function encryptFloodgate (plaintext, key) {
  const iv = crypto.randomBytes(12)
  const cipher = crypto.createCipheriv('aes-128-gcm', key, iv)
  const ciphertext = Buffer.concat([cipher.update(Buffer.from(plaintext, 'utf8')), cipher.final()])
  const tag = cipher.getAuthTag()
  const sealed = Buffer.concat([ciphertext, tag])
  const header = '^Floodgate^' + String.fromCharCode(0x3e) // IDENTIFIER + version 0
  return header + iv.toString('base64') + '!' + sealed.toString('base64')
}

// The twelve `\0`-separated fields `BedrockData.toString()` sends, in order.
function floodgatePlaintext (username, xuid) {
  return [
    '2', username, xuid, '7', 'en_GB', '1', '2', '127.0.0.1', 'null', '0', '123', 'abcdef',
  ].join('\0')
}

function parseLoginFinished (payload) {
  const uuid = formatUuid(payload.slice(0, 16))
  const nameResult = readString(payload, 16)
  if (!nameResult) throw new Error('malformed login-finished packet: no name')
  return { uuid, name: nameResult[0] }
}

function clientInformationPayload () {
  return Buffer.concat([
    writeString('en_us'),
    writeVarint(8), // view distance
    writeVarint(0), // chat visibility: full
    Buffer.from([1]), // chat colors
    writeVarint(0x7f), // displayed skin parts
    writeVarint(1), // main hand: right
    Buffer.from([0]), // text filtering
    Buffer.from([1]), // allow listing
    writeVarint(0), // particle status: all
  ])
}

/** Walks configuration to completion, then waits for the play login packet
 * and at least one chunk -- proof the player is actually seated in the
 * world, not merely accepted at login. */
async function walkConfigAndPlay (connection) {
  connection.send(0, clientInformationPayload()) // S_CLIENT_INFORMATION

  for (;;) {
    const { packetId, payload } = await connection.receive(15000, 'a configuration packet')
    if (packetId === 14) { // C_SELECT_KNOWN_PACKS
      connection.send(7, writeVarint(0)) // S_SELECT_KNOWN_PACKS: claim none
    } else if (packetId === 4) { // C_KEEP_ALIVE
      connection.send(4, payload) // S_KEEP_ALIVE: echo
    } else if (packetId === 5) { // C_PING
      connection.send(5, payload) // S_PONG: echo
    } else if (packetId === 2) { // C_DISCONNECT
      const reason = readString(payload, 0)
      throw new Error(`disconnected during configuration: ${reason ? reason[0] : '(unreadable)'}`)
    } else if (packetId === 3) { // C_FINISH_CONFIGURATION
      connection.send(3) // S_FINISH_CONFIGURATION
      break
    }
  }

  let joined = false
  let chunks = 0
  for (let i = 0; i < 2000 && (!joined || chunks < 1); i++) {
    const { packetId, payload } = await connection.receive(15000, 'a play packet')
    if (packetId === 49) { // C_LOGIN
      joined = true
    } else if (packetId === 45) { // C_LEVEL_CHUNK_WITH_LIGHT
      chunks += 1
    } else if (packetId === 11) { // C_CHUNK_BATCH_FINISHED
      connection.send(11, Buffer.from([0x42, 0x80, 0x00, 0x00])) // S_CHUNK_BATCH_RECEIVED: 64.0f
    } else if (packetId === 32) { // C_DISCONNECT (play)
      throw new Error(`disconnected after login: ${JSON.stringify(payload.slice(0, 200))}`)
    }
  }
  if (!joined) throw new Error('never received the play login packet')
  if (chunks < 1) throw new Error('reached play but received no chunks')
}

/** Sends a Floodgate-carrying handshake plus a Login Start, and returns the
 * derived identity once the server accepts it -- straight off the wire, from
 * `CLoginFinished`, not scraped from a log. */
async function runFloodgate (port, keyPath, username, xuid) {
  const key = fs.readFileSync(keyPath)
  if (key.length !== 16) {
    throw new Error(`${keyPath} is ${key.length} bytes, expected 16 (foton_bedrock::key::KEY_LENGTH)`)
  }

  const payload = encryptFloodgate(floodgatePlaintext(username, xuid), key)
  const hostname = '127.0.0.1\0' + payload

  const connection = await connect(port)

  const portBuf = Buffer.alloc(2)
  portBuf.writeUInt16BE(port)
  connection.send(0, Buffer.concat([
    writeVarint(PROTOCOL_VERSION),
    writeString(hostname), // the handshake hostname: carries the Floodgate payload
    portBuf,
    writeVarint(2), // intention: login
  ]))

  const loginNameBuf = Buffer.from(username.slice(0, 16), 'utf8')
  connection.send(0, Buffer.concat([
    writeVarint(loginNameBuf.length), loginNameBuf,
    Buffer.alloc(16), // profile_id: unread by the Floodgate branch
  ]))

  for (;;) {
    const { packetId, payload: body } = await connection.receive(15000, 'a login response')
    if (packetId === 3) { // C_LOGIN_COMPRESSION
      const thresholdResult = readVarint(body, 0)
      connection.compressionThreshold = thresholdResult ? thresholdResult[0] : -1
      continue
    }
    if (packetId === 2) { // C_LOGIN_FINISHED
      const identity = parseLoginFinished(body)
      connection.send(3) // S_LOGIN_ACKNOWLEDGED
      await walkConfigAndPlay(connection)
      connection.socket.end()
      return identity
    }
    if (packetId === 1) { // C_HELLO -- the ordinary (Mojang) login path
      throw new Error('server asked for encryption instead of accepting the Floodgate handshake')
    }
    if (packetId === 0) { // C_LOGIN_DISCONNECT
      const reason = readString(body, 0)
      throw new Error(`disconnected during login: ${reason ? reason[0] : '(unreadable)'}`)
    }
  }
}

// -- the `join` mode: a real simulated Bedrock client, via Geyser -----------

async function runJoin (port, username) {
  let bedrock
  try {
    bedrock = require('bedrock-protocol')
  } catch {
    console.log('SKIP_NO_BEDROCK_PROTOCOL')
    process.exit(3)
  }

  const client = bedrock.createClient({ host: '127.0.0.1', port, username, offline: true })
  let started = false
  let settled = false

  const finish = (code, label, detail) => {
    if (settled) return
    settled = true
    console.log(detail ? `${label} ${detail}` : label)
    client.close?.()
    process.exit(code)
  }

  client.on('start_game', () => { started = true })
  client.on('play_status', (packet) => {
    if (packet.status === 'player_spawn' && started) finish(0, 'JOINED')
  })
  // A Minecraft-level kick after the Bedrock login chain was accepted.
  client.on('kick', (packet) => finish(1, 'KICKED', JSON.stringify(packet)))
  // RakNet/login-chain rejection -- this is where Geyser's own
  // `validate-bedrock-login` refusal actually surfaces for an
  // unauthenticated client; see the module comment above.
  client.on('close', () => finish(1, 'CLOSED', 'connection closed before spawning'))
  client.on('error', (error) => finish(1, 'ERROR', error.message))
  setTimeout(() => finish(1, 'TIMEOUT'), 20000)
}

// -- entry point --------------------------------------------------------

async function main () {
  const [mode, ...rest] = process.argv.slice(2)
  if (mode === 'join') {
    const [port, username] = rest
    await runJoin(Number(port), username || 'StageZero')
    return
  }
  if (mode === 'floodgate') {
    const [port, keyPath, username, xuid] = rest
    try {
      const identity = await runFloodgate(Number(port), keyPath, username, xuid)
      console.log(`JOINED ${identity.uuid} ${identity.name}`)
      process.exit(0)
    } catch (error) {
      console.error(`FAILED ${error.message}`)
      process.exit(1)
    }
  }
  console.error(`usage: node bedrock-client.js join <port> <username>`)
  console.error(`       node bedrock-client.js floodgate <port> <key.pem> <username> <xuid>`)
  process.exit(2)
}

main()
