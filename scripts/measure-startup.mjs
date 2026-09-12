import { spawn } from 'node:child_process';

const executable = process.argv[2];
const samples = Number(process.argv[3] || 3);
const timeoutMs = 15_000;
if (!executable) throw new Error('usage: node scripts/measure-startup.mjs <executable> [samples]');

function sample() {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, [], { stdio: ['ignore', 'pipe', 'pipe'] });
    let output = '';
    let settled = false;
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      child.kill('SIGTERM');
      error ? reject(error) : resolve(value);
    };
    const onData = chunk => {
      output += chunk.toString();
      const match = output.match(/\[perf\] backend-ready-ms=(\d+)/);
      if (match) finish(null, Number(match[1]));
    };
    child.stdout.on('data', onData);
    child.stderr.on('data', onData);
    child.once('error', error => finish(error));
    child.once('exit', (code, signal) => {
      if (!settled) finish(new Error(`process exited before backend-ready: code=${code} signal=${signal}\n${output}`));
    });
    const timer = setTimeout(() => finish(new Error(`startup timeout after ${timeoutMs}ms\n${output}`)), timeoutMs);
  });
}

const values = [];
for (let i = 0; i < samples; i++) values.push(await sample());
const median = [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
console.log(`backend startup samples: ${values.join(', ')} ms`);
console.log(`backend startup median: ${median} ms`);
