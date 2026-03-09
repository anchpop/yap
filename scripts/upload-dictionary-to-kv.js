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

// Upload per-course data
const coursesData = JSON.parse(fs.readFileSync(path.join(DATA_DIR, 'courses.json'), 'utf-8'));
for (const course of coursesData) {
  const courseSlug = course.slug;
  const courseDir = path.join(DATA_DIR, courseSlug);
  if (!fs.existsSync(courseDir) || !fs.statSync(courseDir).isDirectory()) continue;

  // Upload letters manifest
  const lettersFile = path.join(courseDir, 'letters.json');
  if (fs.existsSync(lettersFile)) {
    console.log(`Uploading letters manifest: ${courseSlug}`);
    wranglerPut(`letters:${courseSlug}`, lettersFile);
  }

  // Upload top-N lists
  for (const n of [100, 1000]) {
    const topFile = path.join(courseDir, `top-${n}.json`);
    if (fs.existsSync(topFile)) {
      console.log(`Uploading top-${n}: ${courseSlug}`);
      wranglerPut(`top:${courseSlug}:${n}`, topFile);
    }
  }

  // Upload per-letter indexes
  const letterDir = path.join(courseDir, 'letter');
  if (fs.existsSync(letterDir)) {
    for (const letterFile of fs.readdirSync(letterDir)) {
      if (!letterFile.endsWith('.json')) continue;
      const letter = letterFile.replace('.json', '');
      wranglerPut(`letter:${courseSlug}:${letter}`, path.join(letterDir, letterFile));
    }
    console.log(`Uploaded letter indexes for ${courseSlug}`);
  }

  // Upload per-page data
  console.log(`Uploading pages for ${courseSlug}...`);
  const pageFiles = fs.readdirSync(courseDir).filter(f => f.endsWith('.json') && f !== 'letters.json' && f !== 'top-100.json' && f !== 'top-1000.json');

  let batch = [];
  let total = 0;

  for (const pageFile of pageFiles) {
    const pageSlug = pageFile.replace('.json', '');
    const value = fs.readFileSync(path.join(courseDir, pageFile), 'utf-8');

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
