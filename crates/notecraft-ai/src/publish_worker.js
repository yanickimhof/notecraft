// NoteCraft publishing Worker: serves the pages stored in the PAGES KV namespace.
// Addresses are /<secret>/..., unknown addresses answer 404, nothing is indexed or cached.
const TYPES = {
  html: 'text/html; charset=utf-8', css: 'text/css', js: 'text/javascript',
  png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp',
  svg: 'image/svg+xml', gif: 'image/gif', pdf: 'application/pdf', woff2: 'font/woff2',
};

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    const base = {
      'Cache-Control': 'no-store',
      'X-Robots-Tag': 'noindex, nofollow',
      'Referrer-Policy': 'no-referrer',
      'X-Content-Type-Options': 'nosniff',
    };
    if (url.pathname === '/robots.txt') {
      return new Response('User-agent: *\nDisallow: /\n', {
        headers: Object.assign({}, base, { 'Content-Type': 'text/plain' }),
      });
    }
    const parts = url.pathname.split('/').filter(Boolean);
    if (parts.length === 0 || !/^[A-Za-z0-9_-]{16,}$/.test(parts[0])) {
      return new Response('Not found', { status: 404, headers: base });
    }
    if (!url.pathname.endsWith('/') && !/\.[A-Za-z0-9]+$/.test(parts[parts.length - 1])) {
      return Response.redirect(url.origin + url.pathname + '/', 301);
    }
    let key = parts.join('/');
    if (url.pathname.endsWith('/')) key += '/index.html';
    const body = await env.PAGES.get(key, 'arrayBuffer');
    if (body === null) return new Response('Not found', { status: 404, headers: base });
    const ext = key.split('.').pop().toLowerCase();
    const headers = Object.assign({}, base, { 'Content-Type': TYPES[ext] || 'application/octet-stream' });
    if (parts[1] === 'a') headers['Cache-Control'] = 'private, max-age=86400';
    return new Response(body, { headers });
  },
};
