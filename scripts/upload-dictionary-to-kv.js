#!/usr/bin/env node
// Upload dictionary JSON data to Cloudflare KV
// Run after: cargo run --release --bin generate-dictionary-data
// Usage: CLOUDFLARE_API_TOKEN=... CLOUDFLARE_ACCOUNT_ID=... node scripts/upload-dictionary-to-kv.js

const fs = require('fs');
const path = require('path');

const DATA_DIR = 'static-site/src/data';
const SEARCH_DIR = 'static-site/public/search';
const KV_NAMESPACE_ID = '4f0627a5baf348e19fd656fc0a3896cd';
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

// Collect all key-value pairs, then flush in batches
let allPairs = [];
let totalUploaded = 0;

function addPair(key, value) {
  allPairs.push({ key, value });
}

function addPairFromFile(key, filePath) {
  addPair(key, fs.readFileSync(filePath, 'utf-8'));
}

async function flushAll() {
  if (allPairs.length === 0) return;

  // Split into batches
  const batches = [];
  for (let i = 0; i < allPairs.length; i += BATCH_SIZE) {
    batches.push(allPairs.slice(i, i + BATCH_SIZE));
  }

  console.log(`Uploading ${allPairs.length} keys in ${batches.length} batches (${CONCURRENCY} concurrent)...`);

  // Upload batches with limited concurrency
  for (let i = 0; i < batches.length; i += CONCURRENCY) {
    const chunk = batches.slice(i, i + CONCURRENCY);
    await Promise.all(chunk.map((batch, j) => {
      const batchNum = i + j + 1;
      console.log(`  Batch ${batchNum}/${batches.length} (${batch.length} keys)...`);
      return kvBulkPut(batch);
    }));
    totalUploaded += chunk.reduce((sum, b) => sum + b.length, 0);
    console.log(`  ${totalUploaded}/${allPairs.length} keys uploaded`);
  }

  allPairs = [];
}

async function main() {
  // Upload courses manifest
  console.log('Collecting courses manifest...');
  addPairFromFile('courses', path.join(DATA_DIR, 'courses.json'));

  // Upload search indexes
  for (const file of fs.readdirSync(SEARCH_DIR)) {
    if (!file.endsWith('.json')) continue;
    const courseSlug = file.replace('.json', '');
    addPairFromFile(`search:${courseSlug}`, path.join(SEARCH_DIR, file));
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
      addPairFromFile(`letters:${courseSlug}`, lettersFile);
    }

    // Upload top-1000 lists (combined, words, phrases)
    for (const variant of ['top-1000', 'top-1000-words', 'top-1000-phrases']) {
      const topFile = path.join(courseDir, `${variant}.json`);
      if (fs.existsSync(topFile)) {
        addPairFromFile(`${variant}:${courseSlug}`, topFile);
      }
    }

    // Upload per-letter indexes
    const letterDir = path.join(courseDir, 'letter');
    if (fs.existsSync(letterDir)) {
      for (const letterFile of fs.readdirSync(letterDir)) {
        if (!letterFile.endsWith('.json')) continue;
        const letter = letterFile.replace('.json', '');
        addPairFromFile(`letter:${courseSlug}:${letter}`, path.join(letterDir, letterFile));
      }
    }

    // Upload per-page data
    console.log(`Collecting pages for ${courseSlug}...`);
    const pageFiles = fs.readdirSync(courseDir).filter(f =>
      f.endsWith('.json') && f !== 'letters.json' && f !== 'top-100.json' && f !== 'top-1000.json'
      && f !== 'top-1000-words.json' && f !== 'top-1000-phrases.json'
    );

    for (const pageFile of pageFiles) {
      const pageSlug = pageFile.replace('.json', '');
      const value = fs.readFileSync(path.join(courseDir, pageFile), 'utf-8');
      addPair(`page:${courseSlug}:${pageSlug}`, value);
    }
  }

  await flushAll();
  console.log(`Done! ${totalUploaded} keys uploaded to KV.`);
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
