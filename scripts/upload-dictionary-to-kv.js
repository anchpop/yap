#!/usr/bin/env node
// Upload dictionary JSON data to Cloudflare KV
// Run after: cargo run --release --bin generate-dictionary-data
// Usage: CLOUDFLARE_API_TOKEN=... CLOUDFLARE_ACCOUNT_ID=... node scripts/upload-dictionary-to-kv.js

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const DATA_DIR = 'static-site/src/data';
const SEARCH_DIR = 'static-site/public/search';
const KV_NAMESPACE_ID = '4f0627a5baf348e19fd656fc0a3896cd';
// Digest of all source files, stored in KV after a successful upload so the
// next run can skip re-uploading identical data (see --check below).
const DIGEST_KEY = 'meta:dictionary-digest';
const BATCH_SIZE = 5000; // Max 10000 per CF API call, but stay conservative on payload size
const MAX_RETRIES = 3;
const CONCURRENCY = 10; // Number of parallel API requests

const API_TOKEN = process.env.CLOUDFLARE_API_TOKEN;
const ACCOUNT_ID = process.env.CLOUDFLARE_ACCOUNT_ID;

if (!API_TOKEN || !ACCOUNT_ID) {
  console.error('CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID must be set');
  process.exit(1);
}

const KV_BULK_URL = `https://api.cloudflare.com/client/v4/accounts/${ACCOUNT_ID}/storage/kv/namespaces/${KV_NAMESPACE_ID}/bulk`;
const KV_VALUE_URL = (key) => `https://api.cloudflare.com/client/v4/accounts/${ACCOUNT_ID}/storage/kv/namespaces/${KV_NAMESPACE_ID}/values/${encodeURIComponent(key)}`;

// Hash every file that main() would upload (path + content, in sorted order).
// If the generator output is byte-identical to what's already in KV, the
// upload can be skipped.
function computeDigest() {
  const hash = crypto.createHash('sha256');
  const walk = (dir) => {
    if (!fs.existsSync(dir)) return;
    const entries = fs.readdirSync(dir, { withFileTypes: true })
      .sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    for (const entry of entries) {
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(p);
      } else {
        hash.update(p);
        hash.update('\0');
        hash.update(fs.readFileSync(p));
        hash.update('\0');
      }
    }
  };
  walk(DATA_DIR);
  walk(SEARCH_DIR);
  return hash.digest('hex');
}

async function fetchRemoteDigest() {
  const res = await fetch(KV_VALUE_URL(DIGEST_KEY), {
    headers: { 'Authorization': `Bearer ${API_TOKEN}` },
  });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`Failed to read ${DIGEST_KEY}: ${res.status} ${await res.text()}`);
  return await res.text();
}

// --check: compare the local digest against KV without uploading anything.
// Writes changed=<bool> to $GITHUB_OUTPUT when running in Actions. Fails open:
// if the KV read errors, report "changed" so the deploy falls back to a full
// upload rather than being blocked.
async function check() {
  const digest = computeDigest();
  let remote = null;
  try {
    remote = await fetchRemoteDigest();
  } catch (err) {
    console.warn(`Could not read ${DIGEST_KEY} (${err.message}); assuming upload is needed`);
  }
  const changed = remote !== digest;
  console.log(`Local digest ${digest}, KV digest ${remote ?? '(none)'} — ${changed ? 'upload needed' : 'data unchanged, upload can be skipped'}`);
  if (process.env.GITHUB_OUTPUT) {
    fs.appendFileSync(process.env.GITHUB_OUTPUT, `changed=${changed}\n`);
  }
}

async function kvBulkPut(pairs, attempt = 1) {
  const res = await fetch(KV_BULK_URL, {
    method: 'PUT',
    headers: {
      'Authorization': `Bearer ${API_TOKEN}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(pairs),
  });

  if (!res.ok) {
    const body = await res.text();
    if (attempt < MAX_RETRIES) {
      console.log(`  Retry ${attempt}/${MAX_RETRIES} (status ${res.status})...`);
      await new Promise(r => setTimeout(r, 1000 * attempt));
      return kvBulkPut(pairs, attempt + 1);
    }
    throw new Error(`KV bulk put failed after ${MAX_RETRIES} attempts: ${res.status} ${body}`);
  }
}

// Streaming batcher: flushes in batches as pairs are added, keeping memory low
let currentBatch = [];
let pendingBatches = [];
let totalCollected = 0;
let totalUploaded = 0;
let totalBatches = 0;

async function addPair(key, value) {
  currentBatch.push({ key, value });
  totalCollected++;
  if (currentBatch.length >= BATCH_SIZE) {
    pendingBatches.push(currentBatch);
    currentBatch = [];
    // Flush immediately when we have enough batches for one concurrent round
    if (pendingBatches.length >= CONCURRENCY) {
      await flushPending();
    }
  }
}

async function addPairFromFile(key, filePath) {
  await addPair(key, fs.readFileSync(filePath, 'utf-8'));
}

async function flushPending() {
  // Flush any partial batch
  if (currentBatch.length > 0) {
    pendingBatches.push(currentBatch);
    currentBatch = [];
  }

  if (pendingBatches.length === 0) return;

  const batches = pendingBatches;
  pendingBatches = [];
  totalBatches += batches.length;

  // Upload batches with limited concurrency
  for (let i = 0; i < batches.length; i += CONCURRENCY) {
    const chunk = batches.slice(i, i + CONCURRENCY);
    await Promise.all(chunk.map((batch, j) => {
      console.log(`  Batch (${batch.length} keys)...`);
      return kvBulkPut(batch);
    }));
    totalUploaded += chunk.reduce((sum, b) => sum + b.length, 0);
    console.log(`  ${totalUploaded} keys uploaded so far`);
  }
}

async function main() {
  // Digest before uploading so the stored value describes exactly the files
  // this run pushed.
  const digest = computeDigest();

  // Upload courses manifest
  console.log('Collecting courses manifest...');
  await addPairFromFile('courses', path.join(DATA_DIR, 'courses.json'));

  // Upload search indexes
  for (const file of fs.readdirSync(SEARCH_DIR)) {
    if (!file.endsWith('.json')) continue;
    const courseSlug = file.replace('.json', '');
    await addPairFromFile(`search:${courseSlug}`, path.join(SEARCH_DIR, file));
  }

  // Upload per-course data
  const coursesData = JSON.parse(fs.readFileSync(path.join(DATA_DIR, 'courses.json'), 'utf-8'));
  for (const course of coursesData) {
    const courseSlug = course.slug;
    const courseDir = path.join(DATA_DIR, courseSlug);
    if (!fs.existsSync(courseDir) || !fs.statSync(courseDir).isDirectory()) continue;

    // Upload letters manifest
    const lettersFile = path.join(courseDir, 'letters.json');
    if (fs.existsSync(lettersFile)) {
      await addPairFromFile(`letters:${courseSlug}`, lettersFile);
    }

    // Upload top-1000 lists (combined, words, phrases)
    for (const variant of ['top-1000', 'top-1000-words', 'top-1000-phrases']) {
      const topFile = path.join(courseDir, `${variant}.json`);
      if (fs.existsSync(topFile)) {
        await addPairFromFile(`${variant}:${courseSlug}`, topFile);
      }
    }

    // Upload per-letter indexes
    const letterDir = path.join(courseDir, 'letter');
    if (fs.existsSync(letterDir)) {
      for (const letterFile of fs.readdirSync(letterDir)) {
        if (!letterFile.endsWith('.json')) continue;
        const letter = letterFile.replace('.json', '');
        await addPairFromFile(`letter:${courseSlug}:${letter}`, path.join(letterDir, letterFile));
      }
    }

    // Upload per-page data, flushing after each course to keep memory bounded
    console.log(`Collecting pages for ${courseSlug}...`);
    const pageFiles = fs.readdirSync(courseDir).filter(f =>
      f.endsWith('.json') && f !== 'letters.json' && f !== 'top-100.json' && f !== 'top-1000.json'
      && f !== 'top-1000-words.json' && f !== 'top-1000-phrases.json'
    );

    for (const pageFile of pageFiles) {
      const pageSlug = pageFile.replace('.json', '');
      const value = fs.readFileSync(path.join(courseDir, pageFile), 'utf-8');
      await addPair(`page:${courseSlug}:${pageSlug}`, value);
    }

    // Flush after each course so we don't accumulate all 200k pairs in memory
    await flushPending();
  }

  // Final flush for any remaining pairs
  await flushPending();

  // Record the digest only after everything uploaded, so a partial failure
  // leaves the old digest in place and the next run re-uploads everything.
  await kvBulkPut([{ key: DIGEST_KEY, value: digest }]);
  console.log(`Done! ${totalUploaded} keys uploaded to KV in ${totalBatches} batches (digest ${digest}).`);
}

(process.argv.includes('--check') ? check() : main()).catch(err => {
  console.error(err);
  process.exit(1);
});
