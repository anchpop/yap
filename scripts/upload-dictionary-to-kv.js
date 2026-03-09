#!/usr/bin/env node
// Upload dictionary JSON data to Cloudflare KV
// Run after: cargo run --release --bin generate-dictionary-data
// Usage: node scripts/upload-dictionary-to-kv.js

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const DATA_DIR = 'dictionary-site/src/data';
const SEARCH_DIR = 'dictionary-site/public/search';
const KV_NAMESPACE_ID = '4f0627a5baf348e19fd656fc0a3896cd';
const BATCH_SIZE = 5000;
const MAX_RETRIES = 3;

function wranglerPut(key, filePath) {
  execSync(`npx wrangler kv key put --remote --namespace-id="${KV_NAMESPACE_ID}" "${key}" --path="${filePath}"`, { stdio: 'inherit' });
}

function wranglerBulkPut(batchFile, attempt = 1) {
  try {
    execSync(`npx wrangler kv bulk put --remote --namespace-id="${KV_NAMESPACE_ID}" "${batchFile}"`, { stdio: 'inherit' });
  } catch (e) {
    if (attempt < MAX_RETRIES) {
      console.log(`  Retry ${attempt}/${MAX_RETRIES}...`);
      wranglerBulkPut(batchFile, attempt + 1);
    } else {
      throw e;
    }
  }
}

// Upload courses manifest
console.log('Uploading courses manifest...');
wranglerPut('courses', path.join(DATA_DIR, 'courses.json'));

// Upload search indexes
for (const file of fs.readdirSync(SEARCH_DIR)) {
  if (!file.endsWith('.json')) continue;
  const courseSlug = file.replace('.json', '');
  console.log(`Uploading search index: ${courseSlug}`);
  wranglerPut(`search:${courseSlug}`, path.join(SEARCH_DIR, file));
}

// Upload course indexes and per-page data
for (const file of fs.readdirSync(DATA_DIR)) {
  if (!file.endsWith('.json') || file === 'courses.json') continue;

  const courseSlug = file.replace('.json', '');
  console.log(`Uploading course index: ${courseSlug}`);
  wranglerPut(`index:${courseSlug}`, path.join(DATA_DIR, file));

  const pageDir = path.join(DATA_DIR, courseSlug);
  if (!fs.existsSync(pageDir) || !fs.statSync(pageDir).isDirectory()) continue;

  console.log(`Uploading pages for ${courseSlug}...`);
  const pageFiles = fs.readdirSync(pageDir).filter(f => f.endsWith('.json'));

  let batch = [];
  let total = 0;

  for (const pageFile of pageFiles) {
    const pageSlug = pageFile.replace('.json', '');
    const value = fs.readFileSync(path.join(pageDir, pageFile), 'utf-8');

    batch.push({ key: `page:${courseSlug}:${pageSlug}`, value });
    total++;

    if (batch.length >= BATCH_SIZE) {
      const tmpFile = path.join(require('os').tmpdir(), `kv-batch-${Date.now()}.json`);
      fs.writeFileSync(tmpFile, JSON.stringify(batch));
      console.log(`  Uploading batch (${total} pages so far)...`);
      wranglerBulkPut(tmpFile);
      fs.unlinkSync(tmpFile);
      batch = [];
    }
  }

  if (batch.length > 0) {
    const tmpFile = path.join(require('os').tmpdir(), `kv-batch-${Date.now()}.json`);
    fs.writeFileSync(tmpFile, JSON.stringify(batch));
    console.log(`  Uploading final batch (${total} pages total)...`);
    wranglerBulkPut(tmpFile);
    fs.unlinkSync(tmpFile);
  }
}

console.log('Done! All dictionary data uploaded to KV.');
