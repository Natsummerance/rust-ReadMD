#!/usr/bin/env node
// Run before replacing a shipped capability lock. Explicitly supply a lock
// from the trusted previous application; never point this at a user runtime.
import path from 'node:path';
import { registerRecoveryLock } from './lib/capability-recovery.mjs';
const [assets, previous] = process.argv.slice(2);
if (!assets || !previous) throw Error('Usage: node tools/register-capability-recovery.mjs <assets> <trusted-previous-lock.json>');
console.log(JSON.stringify(registerRecoveryLock(path.resolve(assets), path.resolve(previous))));
