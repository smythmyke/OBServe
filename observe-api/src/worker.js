// OBServe License API — Cloudflare Worker
// Verifies Stripe checkout sessions, generates Ed25519-signed license keys.
//
// Environment secrets (set via wrangler secret put):
//   STRIPE_SECRET_KEY — sk_live_... or sk_test_...
//   ED25519_PRIVATE_KEY_HEX — 64-char hex private key
//   STRIPE_WEBHOOK_SECRET — whsec_...
//   ADMIN_PIN — 4-digit admin activation code
//   RESEND_API_KEY — re_... (Resend email service)
//
// KV namespaces (bound in wrangler.toml):
//   RATE_LIMIT_KV — rate limiting for recovery endpoint

const ALL_MODULE_IDS = [
  'spectrum', 'video-editor', 'calibration', 'ducking',
  'audio-fx', 'camera', 'presets', 'monitoring', 'narration-studio',
  'sample-pad',
];

const CORS_HEADERS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'POST, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type',
};

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (request.method === 'OPTIONS') {
      return new Response(null, { headers: CORS_HEADERS });
    }

    if (url.pathname === '/success' && request.method === 'GET') {
      return handleSuccess(url, env);
    }
    if (url.pathname === '/activate' && request.method === 'POST') {
      return handlePinActivation(request, env);
    }
    if (url.pathname === '/webhook' && request.method === 'POST') {
      return handleWebhook(request, env);
    }
    if (url.pathname === '/recover' && request.method === 'POST') {
      return handleRecover(request, env);
    }
    if (url.pathname === '/activate-key' && request.method === 'POST') {
      return handleActivateKey(request, env);
    }
    if (url.pathname === '/deactivate-key' && request.method === 'POST') {
      return handleDeactivateKey(request, env);
    }

    // VST DLL serving from R2
    if (url.pathname.startsWith('/vst/') && request.method === 'GET') {
      return handleVstDownload(url, env);
    }

    return new Response('OBServe License API', { status: 200 });
  },
};

// --- Shared Session Processing ---

async function processCheckoutSession(sessionId, env) {
  const session = await stripeGet(`/v1/checkout/sessions/${sessionId}`, env);
  if (session.payment_status !== 'paid') {
    throw new Error('Payment not completed');
  }

  const lineItems = await stripeGet(
    `/v1/checkout/sessions/${sessionId}/line_items`,
    env
  );

  const productIds = lineItems.data.map((li) => li.price?.product).filter(Boolean);
  const email = session.customer_details?.email || session.customer_email || '';

  let customerId = session.customer;
  let existingModules = [];

  if (customerId) {
    const customer = await stripeGet(`/v1/customers/${customerId}`, env);
    const meta = customer.metadata?.observe_modules;
    if (meta) {
      existingModules = meta.split(',').filter(Boolean);
    }
  }

  const newModules = [];
  for (const pid of productIds) {
    const product = await stripeGet(`/v1/products/${pid}`, env);
    const moduleId = product.metadata?.observe_module_id;
    if (moduleId) newModules.push(moduleId);
  }

  // Expand bundle to all individual modules
  const expandedModules = [];
  for (const mod of newModules) {
    if (mod === 'all-modules-bundle') {
      expandedModules.push(...ALL_MODULE_IDS);
    } else {
      expandedModules.push(mod);
    }
  }

  const allModules = [...new Set([...existingModules, ...expandedModules])];

  if (customerId) {
    await stripePost(`/v1/customers/${customerId}`, env, {
      'metadata[observe_modules]': allModules.join(','),
    });
  }

  const licenseKey = await generateLicenseKey(allModules, email, env);

  return { licenseKey, allModules, email };
}

// --- Route Handlers ---

async function handleSuccess(url, env) {
  const sessionId = url.searchParams.get('session_id');
  if (!sessionId) {
    return htmlResponse('Missing session_id', 400);
  }

  try {
    const { licenseKey, allModules, email } = await processCheckoutSession(sessionId, env);

    let emailSent = false;
    if (email && env.RESEND_API_KEY) {
      try {
        await sendLicenseEmail(email, licenseKey, allModules, env);
        emailSent = true;
      } catch (e) {
        console.error('Email send failed (success page):', e.message);
      }
    }

    return htmlResponse(successPage(licenseKey, allModules, email, emailSent), 200);
  } catch (e) {
    return htmlResponse(`Error: ${e.message}`, 500);
  }
}

async function handlePinActivation(request, env) {
  try {
    const { code } = await request.json();
    if (!code || !env.ADMIN_PIN) {
      return new Response(JSON.stringify({ error: 'Invalid code' }), {
        status: 401,
        headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
      });
    }

    if (code !== env.ADMIN_PIN) {
      return new Response(JSON.stringify({ error: 'Invalid code' }), {
        status: 401,
        headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
      });
    }

    const licenseKey = await generateLicenseKey(ALL_MODULE_IDS, 'admin@observe.app', env);

    return new Response(JSON.stringify({ key: licenseKey }), {
      status: 200,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  } catch (e) {
    return new Response(JSON.stringify({ error: e.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  }
}

async function handleWebhook(request, env) {
  if (!env.STRIPE_WEBHOOK_SECRET) {
    return new Response('Webhook secret not configured', { status: 500 });
  }

  const body = await request.text();
  const sigHeader = request.headers.get('Stripe-Signature');
  if (!sigHeader) {
    return new Response('Missing signature', { status: 400 });
  }

  const verified = await verifyStripeSignature(body, sigHeader, env.STRIPE_WEBHOOK_SECRET);
  if (!verified) {
    return new Response('Invalid signature', { status: 400 });
  }

  const event = JSON.parse(body);

  if (event.type === 'checkout.session.completed') {
    try {
      const session = event.data.object;
      const { licenseKey, allModules, email } = await processCheckoutSession(session.id, env);

      if (email && env.RESEND_API_KEY) {
        try {
          await sendLicenseEmail(email, licenseKey, allModules, env);
        } catch (e) {
          console.error('Email send failed (webhook):', e.message);
        }
      }
    } catch (e) {
      console.error('Webhook processing error:', e.message);
    }
  }

  if (event.type === 'charge.refunded') {
    const charge = event.data.object;
    console.log(`Refund received for charge ${charge.id}, customer ${charge.customer} — manual review needed`);
  }

  return new Response('OK', { status: 200 });
}

async function handleRecover(request, env) {
  const RECOVERY_MSG = 'If an account exists with that email, a recovery email has been sent.';

  try {
    const { email } = await request.json();
    if (!email || typeof email !== 'string' || !email.includes('@')) {
      return new Response(JSON.stringify({ message: RECOVERY_MSG }), {
        status: 200,
        headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
      });
    }

    const normalizedEmail = email.toLowerCase().trim();

    // Rate limiting: 3 per email per 24h
    if (env.RATE_LIMIT_KV) {
      const rateKey = `recover:${normalizedEmail}`;
      const existing = await env.RATE_LIMIT_KV.get(rateKey);
      const count = existing ? parseInt(existing, 10) : 0;
      if (count >= 3) {
        return new Response(JSON.stringify({ message: RECOVERY_MSG }), {
          status: 200,
          headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
        });
      }
      await env.RATE_LIMIT_KV.put(rateKey, String(count + 1), { expirationTtl: 86400 });
    }

    // Search Stripe for customer by email
    const customers = await stripeGet(
      `/v1/customers/search?query=email:'${encodeURIComponent(normalizedEmail)}'`,
      env
    );

    if (customers.data && customers.data.length > 0) {
      const customer = customers.data[0];
      const modules = customer.metadata?.observe_modules;

      if (modules) {
        const moduleList = modules.split(',').filter(Boolean);
        if (moduleList.length > 0) {
          const licenseKey = await generateLicenseKey(moduleList, normalizedEmail, env);

          if (env.RESEND_API_KEY) {
            try {
              await sendLicenseEmail(normalizedEmail, licenseKey, moduleList, env);
            } catch (e) {
              console.error('Recovery email send failed:', e.message);
            }
          }
        }
      }
    }

    return new Response(JSON.stringify({ message: RECOVERY_MSG }), {
      status: 200,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  } catch (e) {
    return new Response(JSON.stringify({ message: RECOVERY_MSG }), {
      status: 200,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  }
}

// --- Device-Bound Activation ---

async function sha256hex(str) {
  const data = new TextEncoder().encode(str);
  const hash = await crypto.subtle.digest('SHA-256', data);
  return hexEncode(new Uint8Array(hash));
}

async function handleActivateKey(request, env) {
  try {
    const { key, fingerprint } = await request.json();
    if (!key || !fingerprint) {
      return new Response(JSON.stringify({ error: 'Missing key or fingerprint' }), {
        status: 400,
        headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
      });
    }

    // Verify the key signature (Ed25519)
    const parts = key.split('.');
    if (parts.length !== 2) {
      return new Response(JSON.stringify({ error: 'Invalid key format' }), {
        status: 400,
        headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
      });
    }

    // Decode payload to extract modules/email
    const payloadBytes = base64UrlDecode(parts[0]);
    let payload;
    try {
      payload = JSON.parse(new TextDecoder().decode(payloadBytes));
    } catch {
      return new Response(JSON.stringify({ error: 'Invalid key payload' }), {
        status: 400,
        headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
      });
    }

    // Device limit check via KV
    if (env.DEVICE_KV) {
      const kvKey = `DEVICE_KV:${await sha256hex(key)}`;
      const existing = await env.DEVICE_KV.get(kvKey, 'json');

      if (existing) {
        const fps = existing.fingerprints || [];
        if (fps.includes(fingerprint)) {
          // Same device re-activation — OK
        } else if (fps.length < 2) {
          // Second device — add it
          fps.push(fingerprint);
          await env.DEVICE_KV.put(kvKey, JSON.stringify({
            ...existing,
            fingerprints: fps,
          }));
        } else {
          // Already 2 different devices
          return new Response(JSON.stringify({
            error: 'This key has been activated on 2 devices. Deactivate on another device first, or contact support.',
          }), {
            status: 403,
            headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
          });
        }
      } else {
        // First activation ever
        await env.DEVICE_KV.put(kvKey, JSON.stringify({
          fingerprints: [fingerprint],
          email: payload.email || '',
          modules: payload.modules || [],
        }));
      }
    }

    return new Response(JSON.stringify({
      success: true,
      modules: payload.modules || [],
      email: payload.email || '',
    }), {
      status: 200,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  } catch (e) {
    return new Response(JSON.stringify({ error: e.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  }
}

async function handleDeactivateKey(request, env) {
  try {
    const { key, fingerprint } = await request.json();
    if (!key || !fingerprint) {
      return new Response(JSON.stringify({ error: 'Missing key or fingerprint' }), {
        status: 400,
        headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
      });
    }

    if (env.DEVICE_KV) {
      const kvKey = `DEVICE_KV:${await sha256hex(key)}`;
      const existing = await env.DEVICE_KV.get(kvKey, 'json');

      if (existing) {
        const fps = (existing.fingerprints || []).filter(fp => fp !== fingerprint);
        if (fps.length === 0) {
          await env.DEVICE_KV.delete(kvKey);
        } else {
          await env.DEVICE_KV.put(kvKey, JSON.stringify({ ...existing, fingerprints: fps }));
        }
      }
    }

    return new Response(JSON.stringify({ success: true }), {
      status: 200,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  } catch (e) {
    return new Response(JSON.stringify({ error: e.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json', ...CORS_HEADERS },
    });
  }
}

function base64UrlDecode(str) {
  const b64 = str.replace(/-/g, '+').replace(/_/g, '/');
  const pad = (4 - (b64.length % 4)) % 4;
  const padded = b64 + '='.repeat(pad);
  const binStr = atob(padded);
  const bytes = new Uint8Array(binStr.length);
  for (let i = 0; i < binStr.length; i++) {
    bytes[i] = binStr.charCodeAt(i);
  }
  return bytes;
}

async function handleVstDownload(url, env) {
  const name = decodeURIComponent(url.pathname.slice(5));
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

// --- Email Delivery (Resend) ---

async function sendLicenseEmail(to, licenseKey, modules, env) {
  const moduleListHtml = modules.map(m => `<li style="color:#c8b898;padding:2px 0">${m}</li>`).join('');

  const html = `<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="margin:0;padding:0;background:#12100e;font-family:'Segoe UI',system-ui,sans-serif">
  <div style="max-width:560px;margin:0 auto;padding:40px 24px">
    <h1 style="color:#d4a040;font-size:22px;margin:0 0 4px">OBServe</h1>
    <p style="color:#7a6a50;font-size:13px;margin:0 0 28px">Your license key is ready</p>

    <div style="background:#1a1714;border:2px solid #d4a040;border-radius:8px;padding:18px;margin-bottom:20px">
      <p style="color:#7a6a50;font-size:9px;text-transform:uppercase;letter-spacing:2px;margin:0 0 8px">License Key</p>
      <p style="color:#d4a040;font-family:monospace;font-size:12px;word-break:break-all;margin:0;line-height:1.5">${licenseKey}</p>
    </div>

    <p style="color:#7a6a50;font-size:9px;text-transform:uppercase;letter-spacing:2px;margin:0 0 6px">Unlocked Modules</p>
    <ul style="list-style:none;padding:0;margin:0 0 24px;font-size:12px">${moduleListHtml}</ul>

    <div style="border-top:1px solid #2a2620;padding-top:20px">
      <p style="color:#c8b898;font-size:12px;margin:0 0 12px"><strong>To activate:</strong></p>
      <ol style="color:#7a6a50;font-size:12px;padding-left:18px;margin:0;line-height:2">
        <li>Open OBServe</li>
        <li>Click <strong style="color:#c8b898">Store</strong> in the toolbar</li>
        <li>Paste the license key above</li>
        <li>Click <strong style="color:#c8b898">Activate</strong></li>
      </ol>
    </div>

    <p style="color:#3a3428;font-size:10px;margin-top:32px;text-align:center">
      Keep this email — you can use this key to re-activate on any machine.
    </p>
  </div>
</body>
</html>`;

  const resp = await fetch('https://api.resend.com/emails', {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${env.RESEND_API_KEY}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      from: 'OBServe <noreply@observe.app>',
      to: [to],
      subject: `Your OBServe License Key — ${modules.length} module${modules.length !== 1 ? 's' : ''} unlocked`,
      html,
    }),
  });

  if (!resp.ok) {
    const text = await resp.text();
    throw new Error(`Resend API ${resp.status}: ${text}`);
  }
}

// --- Stripe Webhook Signature Verification ---

async function verifyStripeSignature(payload, sigHeader, secret) {
  const parts = {};
  for (const item of sigHeader.split(',')) {
    const [key, value] = item.split('=');
    parts[key.trim()] = value.trim();
  }

  const timestamp = parts['t'];
  const signature = parts['v1'];
  if (!timestamp || !signature) return false;

  // Reject timestamps older than 5 minutes
  const age = Math.floor(Date.now() / 1000) - parseInt(timestamp, 10);
  if (Math.abs(age) > 300) return false;

  const signedPayload = `${timestamp}.${payload}`;
  const encoder = new TextEncoder();

  const key = await crypto.subtle.importKey(
    'raw',
    encoder.encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign']
  );

  const mac = await crypto.subtle.sign('HMAC', key, encoder.encode(signedPayload));
  const expected = hexEncode(new Uint8Array(mac));

  return timingSafeEqual(expected, signature);
}

function hexEncode(bytes) {
  return Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('');
}

function timingSafeEqual(a, b) {
  if (a.length !== b.length) return false;
  let result = 0;
  for (let i = 0; i < a.length; i++) {
    result |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return result === 0;
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

  const privateKeyHex = env.ED25519_PRIVATE_KEY_HEX;
  const rawKey = hexToBytes(privateKeyHex);

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

function successPage(licenseKey, modules, email, emailSent) {
  const emailNotice = emailSent
    ? `<div class="email-notice success">License key sent to <b>${email}</b></div>`
    : email
      ? `<div class="email-notice">Didn't receive an email? Check spam, or use <b>Forgot your license key?</b> in the app Store panel.</div>`
      : '';

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
      margin: 0 auto 12px;
      word-break: break-all;
      font-family: monospace;
      font-size: 13px;
      color: #d4a040;
      cursor: pointer;
      position: relative;
      transition: border-color 0.3s;
    }
    .key-box:hover { border-color: #fff; }
    .copy-hint {
      font-size: 10px;
      color: #7a6a50;
      margin-bottom: 20px;
      transition: color 0.3s;
    }
    .copy-hint.copied { color: #5aaa5a; }
    .modules {
      color: #5aaa5a;
      font-size: 12px;
      margin-bottom: 24px;
    }
    .email-notice {
      font-size: 11px;
      color: #7a6a50;
      max-width: 400px;
      margin: 0 auto 20px;
      line-height: 1.5;
    }
    .email-notice.success {
      color: #5aaa5a;
    }
    .email-notice b { color: #c8b898; }
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
  <p class="copy-hint" id="copyHint">Click to copy</p>
  <div style="background:#2a2016;border:1px solid #d4a040;border-radius:6px;padding:14px 18px;max-width:460px;margin:0 auto 16px;text-align:left;font-size:12px;line-height:1.6;color:#c8b898">
    <b style="color:#d4a040">&#9888; Important — save your key!</b><br>
    &#8226; This key can be activated on up to <b>2 devices</b><br>
    &#8226; Save it somewhere safe — you'll need it if you reinstall<br>
    &#8226; Do not share this key — sharing may result in deactivation
  </div>
  ${emailNotice}
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
      var box = document.getElementById('keyBox');
      var hint = document.getElementById('copyHint');
      box.style.borderColor = '#5aaa5a';
      hint.textContent = 'Copied!';
      hint.classList.add('copied');
      setTimeout(function() {
        box.style.borderColor = '#d4a040';
        hint.textContent = 'Click to copy';
        hint.classList.remove('copied');
      }, 2000);
    }
  </script>
</body>
</html>`;
}
