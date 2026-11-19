import { syncRelease } from './published-release.mjs';
await syncRelease({ check: process.argv.includes('--check'),
  ...(process.argv.includes('--public-only') ? { folders: ['public'] } : {}) });
