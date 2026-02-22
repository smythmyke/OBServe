// Generate Ed25519 keypair for OBServe license signing.
// Run: node observe-api/scripts/generate-keys.js
//
// Output:
//   Private key (hex) → set as Cloudflare Worker secret: ED25519_PRIVATE_KEY_HEX
//   Public key (base64) → embed in src-tauri/src/store.rs: LICENSE_PUBLIC_KEY_B64

const crypto = require('crypto');

(async () => {
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519');

  const privDer = privateKey.export({ type: 'pkcs8', format: 'der' });
  const pubDer = publicKey.export({ type: 'spki', format: 'der' });

  // Ed25519 raw private key is last 32 bytes of PKCS8 DER
  const privRaw = privDer.slice(-32);
  // Ed25519 raw public key is last 32 bytes of SPKI DER
  const pubRaw = pubDer.slice(-32);

  console.log('=== OBServe Ed25519 License Keys ===\n');
  console.log('Private key (hex) — Cloudflare Worker secret ED25519_PRIVATE_KEY_HEX:');
  console.log(Buffer.from(privRaw).toString('hex'));
  console.log('\nPublic key (base64) — store.rs LICENSE_PUBLIC_KEY_B64:');
  console.log(Buffer.from(pubRaw).toString('base64'));
  console.log('\nPublic key (hex):');
  console.log(Buffer.from(pubRaw).toString('hex'));
})();
