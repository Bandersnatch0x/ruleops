#!/usr/bin/env node
// Protocol-only Node fake Backend Host. Deliberately uses no npm dependency:
// this is a language-neutral contract probe, not a JavaScript rule engine.

const fs = require('fs');
const maxFrameBytes = Number(process.env.FAKE_MAX_FRAME_BYTES || 16777216);
let pending = Buffer.alloc(0), executing = false, cancelled = false, total = 0, fatal = false;
let negotiated = { max: maxFrameBytes, events: 10000, deadline: 0 };
const queue = [];

function vi(n) { n = BigInt(n); const a=[]; do { let b=Number(n&127n); n>>=7n; a.push(b|(n?128:0)); } while(n); return Buffer.from(a); }
function fld(n, b) { return Buffer.concat([vi((n<<3)|2), vi(b.length), b]); }
function u(n, v) { return BigInt(v) === 0n ? Buffer.alloc(0) : Buffer.concat([vi(n<<3), vi(v)]); }
function str(n, s) { return fld(n, Buffer.from(s)); }
function msg(...xs) { return Buffer.concat(xs.filter(Boolean)); }
function field(buf) { const out=[]; let p=0; while(p<buf.length) { let tag=0n,sh=0; do { tag|=BigInt(buf[p]&127)<<BigInt(sh); sh+=7; } while(buf[p++]&128); const n=Number(tag>>3n), w=Number(tag&7n); if(w===0){let v=0n;sh=0;do{v|=BigInt(buf[p]&127)<<BigInt(sh);sh+=7;}while(buf[p++]&128);out.push([n,w,v]);} else if(w===2){let l=0n;sh=0;do{l|=BigInt(buf[p]&127)<<BigInt(sh);sh+=7;}while(buf[p++]&128);l=Number(l);out.push([n,w,buf.subarray(p,p+l)]);p+=l;} else throw Error('unsupported wire type');} return out; }
function one(fs,n) { const x=fs.find(f=>f[0]===n); return x&&x[2]; }
function text(fs,n) { const x=one(fs,n); return x === undefined ? '' : Buffer.from(x).toString(); }
function number(fs,n) { const x=one(fs,n); return x === undefined ? 0 : Number(x); }
function limits(b) { const fs=field(b||Buffer.alloc(0)); return { max:Number(one(fs,1)||0n), events:Number(one(fs,2)||0n), deadline:Number(one(fs,3)||0n) }; }
function decodeEnv(b) { const fs=field(b), out={cid:number(fs,1)}; for(const f of fs){ if(f[0]===2)out.hello=decodeHello(f[2]); if(f[0]===4)out.describe=true; if(f[0]===6)out.plan=field(f[2]); if(f[0]===8)out.execute=field(f[2]); if(f[0]===11)out.cancel=true; } return out; }
function decodeHello(b) { const fs=field(b); return {major:number(fs,1), required:fs.filter(x=>x[0]===3).map(x=>Buffer.from(x[2]).toString()), lim:limits(one(fs,5))}; }
function send(b) {
  if (b.length > negotiated.max) {
    process.stderr.write('[node-fake-host] outbound frame exceeds negotiated max ' + negotiated.max + '\n');
    fatal = true;
    setTimeout(() => process.exit(1), 0);
    return;
  }
  process.stdout.write(Buffer.concat([Buffer.from([(b.length>>>24)&255,(b.length>>>16)&255,(b.length>>>8)&255,b.length&255]),b]));
}
function env(cid, n, body) { return msg(u(1,cid), fld(n,body)); }
function mapEntry(k,v) { return msg(str(1,k),u(2,v?1:0)); }
function caps() { return msg(str(1,'java'), fld(2,mapEntry('java.ast',true)), str(3,'SymbolRule'), str(4,'fake-node-backend'), str(5,'0.1.0')); }
function ack(cid, h) {
  negotiated = {
    max: Math.min(h.lim.max || maxFrameBytes, maxFrameBytes),
    events: Math.min(h.lim.events || 10000, 10000),
    deadline: h.lim.deadline || 0,
  };
  const lim = msg(u(1, negotiated.max), u(2, negotiated.events), u(3, negotiated.deadline));
  const a = msg(u(1,h.major),u(2,1),str(3,'erf-node-fake-host'),str(4,'0.1.0'),fld(5,caps()),fld(6,lim));
  return env(cid,3,a);
}
function error(cid, code, detail) { return env(cid,14,msg(u(1,code),str(2,detail))); }
function fatalError(code, detail) { if (fatal) return; fatal = true; send(error(0, code, detail)); setTimeout(() => process.exit(1), 0); }
function parseUnits(fs) { return fs.filter(x=>x[0]===1).map(x=>field(x[2])); }
function plan(cid, fs) { const units=parseUnits(fs); return env(cid,7,msg(...units.map(u0=>fld(1,msg(str(1,text(u0,1)),str(2,'batch-0')))))); }
function observation(cid,id) { const span=msg(str(1,'src/main/java/A.java'),fld(2,Buffer.alloc(32)),u(3,0),u(4,17),u(5,1),u(6,5)); const loc=fld(1,span); const o=msg(str(1,'corp.java.no-system-exit'),fld(2,loc),str(3,`canned observation in ${id}`),str(4,'error')); return env(cid,9,fld(1,o)); }
function status(cid,id,outcome=0,reason=0,detail='') { const s=msg(str(1,id),u(2,outcome),u(3,reason),str(4,detail)); return env(cid,9,fld(2,s)); }
function completion(statusCode,reason) { return env(0,13,msg(u(1,statusCode),str(2,reason),u(3,total))); }
function sleep(ms) { return new Promise(r=>setTimeout(r,ms)); }
async function execute(cid, fs) {
  executing=true; cancelled=false;
  const ids=fs.filter(x=>x[0]===1).map(x=>Buffer.from(x[2]).toString());
  const expectedEvents = ids.length * 2;
  if (expectedEvents > negotiated.events) {
    send(error(cid,6,'execute event count '+expectedEvents+' exceeds negotiated max '+negotiated.events));
    send(completion(1,'event limit exceeded'));
    executing=false;
    return;
  }
  const started=Date.now();
  if(process.env.FAKE_CRASH_ON_EXECUTE){ process.stderr.write('[node-fake-host] injected crash during execute\n'); process.exit(17); }
  for(const id of ids){
    if(Number(process.env.FAKE_DELAY_MS||process.env.ERF_DELAY_MS||0)>0) await sleep(Number(process.env.FAKE_DELAY_MS||process.env.ERF_DELAY_MS));
    if(cancelled){ send(status(cid,id,1,1,'cancelled before completion')); total++; send(completion(1,'cancelled by runtime')); executing=false; return; }
    if(negotiated.deadline && Date.now()-started >= negotiated.deadline){ send(status(cid,id,1,2,'deadline exceeded')); total++; send(completion(1,'deadline exceeded')); executing=false; return; }
    send(observation(cid,id)); send(status(cid,id)); total++;
  }
  send(env(cid,10,u(1,ids.length))); executing=false;
}
async function dispatch(raw) { const e=decodeEnv(raw); if(e.hello){ if(e.hello.major!==Number(process.env.FAKE_SUPPORTED_MAJOR||0)) return send(error(e.cid,3,'protocol major mismatch')); if(e.hello.required.some(x=>x!=='java.ast')) return send(error(e.cid,4,'missing required capability')); return send(ack(e.cid,e.hello)); } if(e.describe)return send(env(e.cid,5,fld(1,caps()))); if(e.plan)return send(plan(e.cid,e.plan)); if(e.cancel){ if(!executing)return send(error(e.cid,6,'cancel is only valid during execution')); cancelled=true; return send(env(e.cid,12,u(1,1))); } if(e.execute){ if(executing)return send(error(e.cid,6,'execute is already in flight')); return execute(e.cid,e.execute); } return send(error(e.cid,5,'unexpected request')); }
process.stdin.on('data', chunk=>{
  if (fatal) return;
  pending=Buffer.concat([pending,chunk]);
  while(pending.length>=4){
    const n=pending.readUInt32BE(0);
    if(n > maxFrameBytes){ fatalError(2,`frame size ${n} exceeds max ${maxFrameBytes}`); return; }
    if(pending.length<4+n)break;
    const b=pending.subarray(4,4+n); pending=pending.subarray(4+n);
    queue.push(dispatch(b).catch(error=>{ process.stderr.write(String(error)); process.exit(1); }));
  }
});
process.stdin.on('end',async()=>{await Promise.all(queue); if(!executing && !fatal){send(completion(0,'session end'));process.exit(0);} });
