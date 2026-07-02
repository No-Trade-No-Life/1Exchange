import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { execSync } from 'node:child_process';
import path from 'node:path';
import packageJson from './package.json' with { type: 'json' };

function commandOutput(command: string) {
  try {
    return execSync(command, { encoding: 'utf8' }).trim();
  } catch {
    return 'unknown';
  }
}

function buildInfoPlugin() {
  const buildInfo = {
    package_version: packageJson.version,
    git_sha: process.env.GITHUB_SHA ?? commandOutput('git rev-parse HEAD'),
    git_ref: process.env.GITHUB_REF_NAME ?? 'unknown',
    build_time: process.env.BUILD_TIME ?? new Date().toISOString(),
    github_run_id: process.env.GITHUB_RUN_ID ?? 'unknown',
  };

  return {
    name: 'one-exchange-build-info',
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'build-info.json',
        source: JSON.stringify(buildInfo, null, 2) + '\n',
      });
    },
  };
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');

  return {
    plugins: [react(), tailwindcss(), buildInfoPlugin()],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
    server: {
      proxy: {
        '/api': env.VITE_API_TARGET ?? 'http://127.0.0.1:8787',
      },
    },
  };
});
