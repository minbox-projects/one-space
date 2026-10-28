#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(scriptDir, '..', '..');
const manifestPath = 'src-tauri/zen-relay/Cargo.toml';
const binaryName = 'onespace-zen-relay';

function fallbackHostTriple() {
  const arch =
    process.arch === 'x64'
      ? 'x86_64'
      : process.arch === 'arm64'
        ? 'aarch64'
        : process.arch;

  if (process.platform === 'darwin') return `${arch}-apple-darwin`;
  if (process.platform === 'win32') return `${arch}-pc-windows-msvc`;
  if (process.platform === 'linux') return `${arch}-unknown-linux-gnu`;

  throw new Error(
    `cannot infer a target triple for ${process.platform}/${process.arch}; set ONESPACE_TARGET_TRIPLE`,
  );
}

function detectHostTriple() {
  const result = spawnSync('rustc', ['-vV'], { encoding: 'utf8' });
  if (result.status === 0 && result.stdout) {
    const hostLine = result.stdout
      .split(/\r?\n/)
      .find((line) => line.startsWith('host:'));
    if (hostLine) {
      const host = hostLine.slice('host:'.length).trim();
      if (host) return host;
    }
  }
  return fallbackHostTriple();
}

function envTriple(name) {
  const value = process.env[name];
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function parseMode(args) {
  if (args.length !== 1 || (args[0] !== '--dev' && args[0] !== '--release')) {
    throw new Error(
      'usage: node src-tauri/scripts/build-zen-relay.mjs <--dev|--release>',
    );
  }
  return args[0] === '--release';
}

function main() {
  const isRelease = parseMode(process.argv.slice(2));
  const hostTriple = detectHostTriple();
  const targetTriple =
    envTriple('ONESPACE_TARGET_TRIPLE') ??
    envTriple('TAURI_ENV_TARGET_TRIPLE') ??
    hostTriple;

  const profile = isRelease ? 'release' : 'debug';
  const useTargetFlag = isRelease || targetTriple !== hostTriple;
  const isWindowsTarget = targetTriple.includes('windows');
  const artifactName = `${binaryName}${isWindowsTarget ? '.exe' : ''}`;

  const cargoArgs = ['build', '--manifest-path', manifestPath];
  if (isRelease) cargoArgs.push('--release');
  if (useTargetFlag) cargoArgs.push('--target', targetTriple);

  const build = spawnSync('cargo', cargoArgs, {
    cwd: repoRoot,
    stdio: 'inherit',
    env: process.env,
  });
  if (build.error) {
    throw new Error(`failed to run cargo: ${build.error.message}`);
  }
  if (build.status !== 0) {
    throw new Error(
      `cargo build failed with exit code ${build.status ?? 'unknown'}`,
    );
  }

  const targetRoot = join(repoRoot, 'src-tauri', 'zen-relay', 'target');
  const artifactPath = useTargetFlag
    ? join(targetRoot, targetTriple, profile, artifactName)
    : join(targetRoot, profile, artifactName);

  if (!existsSync(artifactPath)) {
    throw new Error(`expected sidecar artifact was not produced: ${artifactPath}`);
  }
  console.log(`[zen-relay] built ${profile} artifact: ${artifactPath}`);

  const binariesDir = join(repoRoot, 'src-tauri', 'binaries');
  const destination = join(
    binariesDir,
    `${binaryName}-${targetTriple}${isWindowsTarget ? '.exe' : ''}`,
  );
  mkdirSync(binariesDir, { recursive: true });
  copyFileSync(artifactPath, destination);
  if (process.platform !== 'win32') chmodSync(destination, 0o755);
  console.log(`[zen-relay] copied ${profile} artifact: ${destination}`);
}

try {
  main();
} catch (error) {
  console.error(
    `[zen-relay] ${error instanceof Error ? error.message : String(error)}`,
  );
  process.exit(1);
}
