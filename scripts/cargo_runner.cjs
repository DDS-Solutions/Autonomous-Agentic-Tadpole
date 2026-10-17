#!/usr/bin/env node
/**
 * scripts/cargo_runner.js
 * Runs cargo commands safely, clearing RUSTC_WRAPPER=sccache if sccache is missing on host.
 */
const { spawnSync } = require('child_process');

const env = { ...process.env };
if (env.RUSTC_WRAPPER === 'sccache') {
    const isWindows = process.platform === 'win32';
    const checkCmd = isWindows ? 'where sccache' : 'which sccache';
    const result = spawnSync(checkCmd, { shell: true, stdio: 'ignore' });
    if (result.status !== 0) {
        delete env.RUSTC_WRAPPER;
    }
}

const args = process.argv.slice(2);
const res = spawnSync('cargo', args, { env, stdio: 'inherit', shell: true });
process.exit(res.status ?? 0);
