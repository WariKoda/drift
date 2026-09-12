// Optional Sites deployment bundle. The website itself needs no JavaScript.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
const base = new URL('./', import.meta.url);
const assets = {};
for (const [path, type] of Object.entries({
  'index.html': 'text/html; charset=utf-8',
  'styles.css': 'text/css; charset=utf-8',
  'assets/drift-browser.png': 'image/png',
})) {
  assets['/' + path] = { type, body: (await readFile(new URL(path, base))).toString('base64') };
}
const source = `const assets = ${JSON.stringify(assets)};
export default {
  fetch(request) {
    if (!['GET', 'HEAD'].includes(request.method)) return new Response('Method not allowed', { status: 405, headers: { Allow: 'GET, HEAD' } });
    const path = new URL(request.url).pathname;
    const asset = assets[path === '/' ? '/index.html' : path];
    if (!asset) return new Response('Not found', { status: 404 });
    const headers = { 'Content-Type': asset.type, 'X-Content-Type-Options': 'nosniff', 'Cache-Control': 'public, max-age=300' };
    return new Response(request.method === 'HEAD' ? null : Uint8Array.from(atob(asset.body), c => c.charCodeAt(0)), { headers });
  }
};
`;
await mkdir(new URL('dist/server/', base), { recursive: true });
await writeFile(new URL('dist/server/index.js', base), source);
