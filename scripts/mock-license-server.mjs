import { createHash, createPrivateKey, sign } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { createServer } from 'node:http'

const port = Number(process.env.BARD_MOCK_LICENSE_PORT ?? 8787)
const privateKeyPath = process.env.BARD_LICENSE_PRIVATE_KEY_PATH
if (!privateKeyPath) throw new Error('Set BARD_LICENSE_PRIVATE_KEY_PATH to the development signing key')
const privateKey = createPrivateKey(readFileSync(privateKeyPath, 'utf8'))
const activationId = '00000000-0000-4000-8000-000000000001'

function canonicalJson(value) {
  if (value === null || typeof value !== 'object') return JSON.stringify(value)
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`
}

function response(req, res, status, body) {
  res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' })
  res.end(JSON.stringify(body))
}

function receipt(key, machineId) {
  const now = Math.floor(Date.now() / 1000)
  const claims = {
    activation_id: activationId,
    entitlement: 'active',
    issued_at: now,
    key_fingerprint: createHash('sha256').update(key).digest('hex'),
    key_id: '2026-01',
    lease_sequence: 1,
    lease_version: 1,
    license_expires_at: now + 365 * 24 * 60 * 60,
    machine_id: machineId,
    next_check_at: now + 20 * 60 * 60,
    product: 'bard',
    valid_until: now + 30 * 24 * 60 * 60,
    v: 1,
  }
  const payload = Buffer.from(canonicalJson(claims), 'utf8')
  return `${payload.toString('base64url')}.${sign(null, payload, privateKey).toString('base64url')}`
}

createServer((req, res) => {
  if (req.method !== 'POST' || !req.url?.endsWith('/activate') && !req.url?.endsWith('/validate') && !req.url?.endsWith('/deactivate')) {
    return response(req, res, 404, { ok: false, message: 'Not found' })
  }
  let raw = ''
  req.setEncoding('utf8')
  req.on('data', (chunk) => { raw += chunk })
  req.on('end', () => {
    try {
      const body = JSON.parse(raw)
      if (typeof body.key !== 'string' || typeof body.machineId !== 'string') return response(req, res, 400, { ok: false, message: 'Invalid test request' })
      if (req.url.endsWith('/deactivate')) return response(req, res, 200, { ok: true })
      return response(req, res, 200, { ok: true, receipt: receipt(body.key, body.machineId) })
    } catch {
      return response(req, res, 400, { ok: false, message: 'Invalid JSON' })
    }
  })
}).listen(port, '127.0.0.1', () => {
  console.log(`Mock Bard licence server listening on http://127.0.0.1:${port}`)
})
