// Exercise the shipped API, auth, and extractor against a deterministic upstream fixture.
// The hook is created in a temporary directory and is never included in the archive.
import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createServer } from 'node:net';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
const bundle = resolve(process.argv[2]);
const temp = mkdtempSync(join(tmpdir(), 'lumen-cobalt-smoke-'));
const key = randomUUID();
writeFileSync(join(temp, 'keys.json'), JSON.stringify({ [key]: { limit: 'unlimited' } }));
writeFileSync(join(temp, 'fixture.mjs'), `const original = globalThis.fetch; globalThis.fetch = (url, ...args) => String(url) === 'https://api.streamable.com/videos/lumen1' ? Promise.resolve(Response.json({title:'Lumen fixture',files:{mp4:{url:'https://example.com/fixture.mp4',width:640,height:360}}})) : original(url,...args);`);
const server = createServer();
await new Promise(r => server.listen(0, '127.0.0.1', r));
const port = server.address().port;
await new Promise(r => server.close(r));
const url = `http://127.0.0.1:${port}/`;
const child = spawn(join(bundle, 'node.exe'), ['--no-node-snapshot', '--import', pathToFileURL(join(temp, 'fixture.mjs')).href, 'src/cobalt.js'], {
  cwd: join(bundle, 'api'), stdio: 'ignore', env: { ...process.env, API_URL:url, API_PORT:String(port), API_LISTEN_ADDRESS:'127.0.0.1', API_AUTH_REQUIRED:'1', API_KEY_URL:pathToFileURL(join(temp,'keys.json')).href, CORS_WILDCARD:'0', CORS_URL:url }
});
try {
  let ready = false;
  for (let i=0;i<75;i++) { try { ready = (await fetch(url,{signal:AbortSignal.timeout(1000)})).ok; } catch {} if (ready) break; await new Promise(r=>setTimeout(r,200)); }
  assert(ready, 'companion readiness');
  const request = {signal:AbortSignal.timeout(5000),method:'POST', headers:{Accept:'application/json','Content-Type':'application/json'}, body:JSON.stringify({url:'https://streamable.com/lumen1',localProcessing:'disabled'})};
  const unauthorized = await fetch(url,request);
  assert.equal(unauthorized.status,401);
  const response = await fetch(url,{...request, headers:{...request.headers,Authorization:`Api-Key ${key}`}});
  const result = await response.json();
  assert.equal(result.status,'redirect',JSON.stringify(result));
  assert.equal(result.url,'https://example.com/fixture.mp4');
  console.log('Companion archive: startup, authentication and fixture extraction passed');
} finally {
  if (child.pid) { try { execFileSync('taskkill',['/F','/T','/PID',String(child.pid)],{stdio:'ignore'}); } catch {} }
  await new Promise(r => (child.exitCode !== null || child.signalCode !== null) ? r() : child.once('exit',r));
  rmSync(temp,{recursive:true,force:true});
}
