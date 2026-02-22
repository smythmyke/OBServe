// OBServe License API — Cloudflare Worker
// Verifies Stripe checkout sessions, generates Ed25519-signed license keys.
//
// Environment secrets (set via wrangler secret put):
//   STRIPE_SECRET_KEY — sk_live_... or sk_test_...
//   ED25519_PRIVATE_KEY_HEX — 64-char hex private key
//   STRIPE_WEBHOOK_SECRET — whsec_... (optional)
//   ADMIN_PIN — 4-digit admin activation code

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === '/success' && request.method === 'GET') {
      return handleSuccess(url, env);
    }
    if (url.pathname === '/activate' && request.method === 'OPTIONS') {
      return new Response(null, {
        headers: {
          'Access-Control-Allow-Origin': '*',
          'Access-Control-Allow-Methods': 'POST, OPTIONS',
          'Access-Control-Allow-Headers': 'Content-Type',
        },
      });
    }
    if (url.pathname === '/activate' && request.method === 'POST') {
      return handlePinActivation(request, env);
    }
    if (url.pathname === '/webhook' && request.method === 'POST') {
      return handleWebhook(request, env);
    }

    // VST DLL serving from R2
    if (url.pathname.startsWith('/vst/') && request.method === 'GET') {
      return handleVstDownload(url, env);
    }
    if (url.pathname.startsWith('/vst/') && request.method === 'OPTIONS') {
      return new Response(null, {
        headers: {
          'Access-Control-Allow-Origin': '*',
          'Access-Control-Allow-Methods': 'GET, OPTIONS',
          'Access-Control-Allow-Headers': 'Content-Type',
        },
      });
    }

    return new Response('OBServe License API', { status: 200 });
  },
};

async function handleSuccess(url, env) {
  const sessionId = url.searchParams.get('session_id');
  if (!sessionId) {
    return htmlResponse('Missing session_id', 400);
  }

  try {
    // Verify session with Stripe
    const session = await stripeGet(`/v1/checkout/sessions/${sessionId}`, env);
    if (session.payment_status !== 'paid') {
      return htmlResponse('Payment not completed', 400);
    }

    // Get customer and their purchased product
    const lineItems = await stripeGet(
      `/v1/checkout/sessions/${sessionId}/line_items`,
      env
    );

    const productIds = lineItems.data.map((li) => li.price?.product).filter(Boolean);
    const email = session.customer_details?.email || session.customer_email || '';

    // Look up customer to get existing modules
    let customerId = session.customer;
    let existingModules = [];

    if (customerId) {
      const customer = await stripeGet(`/v1/customers/${customerId}`, env);
      const meta = customer.metadata?.observe_modules;
      if (meta) {
        existingModules = meta.split(',').filter(Boolean);
      }
    }

    // Map Stripe product IDs to module IDs
    const newModules = [];
    for (const pid of productIds) {
      const product = await stripeGet(`/v1/products/${pid}`, env);
      const moduleId = product.metadata?.observe_module_id;
      if (moduleId) newModules.push(moduleId);
    }

    // Merge existing + new modules (deduplicated)
    const allModules = [...new Set([...existingModules, ...newModules])];

    // Update customer metadata
    if (customerId) {
      await stripePost(`/v1/customers/${customerId}`, env, {
        'metadata[observe_modules]': allModules.join(','),
      });
    }

    // Generate signed license key
    const licenseKey = await generateLicenseKey(allModules, email, env);

    return htmlResponse(successPage(licenseKey, allModules, email), 200);
  } catch (e) {
    return htmlResponse(`Error: ${e.message}`, 500);
  }
}

async function handlePinActivation(request, env) {
  const corsHeaders = {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
  };

  try {
    const { code } = await request.json();
    if (!code || !env.ADMIN_PIN) {
      return new Response(JSON.stringify({ error: 'Invalid code' }), {
        status: 401,
        headers: { 'Content-Type': 'application/json', ...corsHeaders },
      });
    }

    if (code !== env.ADMIN_PIN) {
      return new Response(JSON.stringify({ error: 'Invalid code' }), {
        status: 401,
        headers: { 'Content-Type': 'application/json', ...corsHeaders },
      });
    }

    const allModules = [
      'spectrum', 'video-editor', 'calibration', 'ducking',
      'audio-fx', 'camera', 'presets', 'monitoring',
    ];
    const licenseKey = await generateLicenseKey(allModules, 'admin@observe.app', env);

    return new Response(JSON.stringify({ key: licenseKey }), {
      status: 200,
      headers: { 'Content-Type': 'application/json', ...corsHeaders },
    });
  } catch (e) {
    return new Response(JSON.stringify({ error: e.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json', ...corsHeaders },
    });
  }
}

async function handleWebhook(request, env) {
  // Optional: handle checkout.session.completed for email delivery backup
  return new Response('OK', { status: 200 });
}

async function handleVstDownload(url, env) {
  const name = decodeURIComponent(url.pathname.slice(5)); // e.g. "Tape.dll"
  if (!name || !name.endsWith('.dll') || name.includes('/') || name.includes('\\')) {
    return new Response('Invalid plugin name', { status: 400 });
  }
  if (!env.VST_BUCKET) {
    return new Response('VST storage not configured', { status: 503 });
  }
  const object = await env.VST_BUCKET.get(name);
  if (!object) {
    return new Response('Plugin not found', { status: 404 });
  }
  return new Response(object.body, {
    headers: {
      'Content-Type': 'application/octet-stream',
      'Content-Disposition': `attachment; filename="${name}"`,
      'Access-Control-Allow-Origin': '*',
      'Cache-Control': 'public, max-age=86400',
    },
  });
}

// --- Stripe Helpers ---

async function stripeGet(path, env) {
  const resp = await fetch(`https://api.stripe.com${path}`, {
    headers: { Authorization: `Bearer ${env.STRIPE_SECRET_KEY}` },
  });
  if (!resp.ok) throw new Error(`Stripe ${path}: ${resp.status}`);
  return resp.json();
}

async function stripePost(path, env, body) {
  const resp = await fetch(`https://api.stripe.com${path}`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${env.STRIPE_SECRET_KEY}`,
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: new URLSearchParams(body).toString(),
  });
  if (!resp.ok) throw new Error(`Stripe POST ${path}: ${resp.status}`);
  return resp.json();
}

// --- License Key Generation ---

async function generateLicenseKey(modules, email, env) {
  const payload = JSON.stringify({
    modules,
    email,
    ts: Math.floor(Date.now() / 1000),
  });

  const payloadBytes = new TextEncoder().encode(payload);

  // Import Ed25519 private key (raw 32 bytes → PKCS8 wrapper for WebCrypto)
  const privateKeyHex = env.ED25519_PRIVATE_KEY_HEX;
  const rawKey = hexToBytes(privateKeyHex);

  // PKCS8 DER prefix for Ed25519: SEQUENCE { version(0), AlgorithmIdentifier(Ed25519), OCTET_STRING { key } }
  const pkcs8Prefix = new Uint8Array([
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06,
    0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
  ]);
  const pkcs8 = new Uint8Array(pkcs8Prefix.length + rawKey.length);
  pkcs8.set(pkcs8Prefix);
  pkcs8.set(rawKey, pkcs8Prefix.length);

  const algorithm = { name: 'NODE-ED25519', namedCurve: 'NODE-ED25519' };

  const key = await crypto.subtle.importKey('pkcs8', pkcs8, algorithm, false, ['sign']);
  const signature = await crypto.subtle.sign(algorithm, key, payloadBytes);

  // Encode as URL-safe base64: payload.signature
  const payloadB64 = base64UrlEncode(payloadBytes);
  const sigB64 = base64UrlEncode(new Uint8Array(signature));

  return `${payloadB64}.${sigB64}`;
}

// --- Encoding Helpers ---

function hexToBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    bytes[i / 2] = parseInt(hex.substring(i, i + 2), 16);
  }
  return bytes;
}

function base64UrlEncode(bytes) {
  const binStr = Array.from(bytes, (b) => String.fromCharCode(b)).join('');
  return btoa(binStr).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

// --- HTML Helpers ---

function htmlResponse(body, status) {
  return new Response(
    typeof body === 'string' && !body.startsWith('<!DOCTYPE')
      ? `<!DOCTYPE html><html><head><meta charset="utf-8"><title>OBServe</title>
         <style>body{background:#12100e;color:#c8b898;font-family:monospace;padding:40px;text-align:center}</style>
         </head><body><p>${body}</p></body></html>`
      : body,
    {
      status,
      headers: { 'Content-Type': 'text/html;charset=utf-8' },
    }
  );
}

function successPage(licenseKey, modules, email) {
  return `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>OBServe — Purchase Complete</title>
  <style>
    body {
      background: #12100e;
      color: #c8b898;
      font-family: 'Segoe UI', monospace;
      padding: 40px 20px;
      text-align: center;
    }
    h1 { color: #d4a040; font-size: 24px; margin-bottom: 8px; }
    .subtitle { color: #7a6a50; font-size: 14px; margin-bottom: 32px; }
    .key-box {
      background: #1a1714;
      border: 2px solid #d4a040;
      border-radius: 8px;
      padding: 20px;
      max-width: 500px;
      margin: 0 auto 24px;
      word-break: break-all;
      font-family: monospace;
      font-size: 13px;
      color: #d4a040;
      cursor: pointer;
      position: relative;
    }
    .key-box:hover { border-color: #fff; }
    .key-box::after {
      content: 'Click to copy';
      position: absolute;
      bottom: -20px;
      left: 50%;
      transform: translateX(-50%);
      font-size: 10px;
      color: #7a6a50;
    }
    .modules {
      color: #5aaa5a;
      font-size: 12px;
      margin-bottom: 24px;
    }
    .instructions {
      color: #7a6a50;
      font-size: 12px;
      line-height: 1.6;
      max-width: 400px;
      margin: 0 auto;
    }
    .instructions b { color: #c8b898; }
  </style>
</head>
<body>
  <h1>Thank You!</h1>
  <p class="subtitle">Your OBServe purchase is complete</p>
  <p class="modules">Modules: ${modules.join(', ')}</p>
  <div class="key-box" id="keyBox" onclick="copyKey()">${licenseKey}</div>
  <div class="instructions">
    <b>To activate:</b><br>
    1. Open OBServe<br>
    2. Click <b>Store</b> in the toolbar<br>
    3. Paste the key above into the activation field<br>
    4. Click <b>Activate</b>
  </div>
  <script>
    function copyKey() {
      navigator.clipboard.writeText(document.getElementById('keyBox').textContent);
      document.getElementById('keyBox').style.borderColor = '#5aaa5a';
      setTimeout(() => { document.getElementById('keyBox').style.borderColor = '#d4a040'; }, 1500);
    }
  </script>
</body>
</html>`;
}
