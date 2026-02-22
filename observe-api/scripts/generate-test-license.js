// Generate a test license key for development.
// Run: node observe-api/scripts/generate-test-license.js [module1,module2,...]
//
// This generates a keypair, creates a signed license, and outputs:
//   1. The license key (paste into OBServe Store activation)
//   2. The public key (update store.rs LICENSE_PUBLIC_KEY_B64)

const crypto = require('crypto');

const modules = (process.argv[2] || 'spectrum,video-editor,calibration,ducking,audio-fx,camera,presets,monitoring').split(',');
const email = process.argv[3] || 'test@observe.dev';

const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');

const payload = JSON.stringify({
  modules,
  email,
  ts: Math.floor(Date.now() / 1000),
});

const payloadBytes = Buffer.from(payload);
const signature = crypto.sign(null, payloadBytes, privateKey);

const payloadB64 = payloadBytes.toString('base64url');
const sigB64 = signature.toString('base64url');
const licenseKey = `${payloadB64}.${sigB64}`;

const pubDer = publicKey.export({ type: 'spki', format: 'der' });
const pubRaw = pubDer.slice(-32);
const pubB64 = Buffer.from(pubRaw).toString('base64');

console.log('=== Test License ===\n');
console.log('Modules:', modules.join(', '));
console.log('Email:', email);
console.log('\n--- License Key (paste into OBServe) ---');
console.log(licenseKey);
console.log('\n--- Public Key (update store.rs LICENSE_PUBLIC_KEY_B64) ---');
console.log(pubB64);
console.log('\nIMPORTANT: Update LICENSE_PUBLIC_KEY_B64 in src-tauri/src/store.rs');
console.log('then rebuild the app for this test license to work.');
