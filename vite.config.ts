import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

let buildCommit = '未知';
try {
  const options = {
    cwd: fileURLToPath(new URL('.', import.meta.url)),
    encoding: 'utf8' as const,
    stdio: ['ignore', 'pipe', 'ignore'] as ['ignore', 'pipe', 'ignore'],
  };
  const commit = execFileSync('git', ['rev-parse', '--short=7', 'HEAD'], options).trim();
  const modified = execFileSync('git', ['status', '--porcelain'], options).trim();
  buildCommit = commit + (modified ? '（本地修改）' : '');
} catch {
  // Source archives can be built without Git metadata; never invent a commit ID.
}

export default defineConfig({
  plugins: [react()],
  define: { __BUILD_COMMIT__: JSON.stringify(buildCommit) },
  server: { port: 1420, strictPort: true },
  clearScreen: false,
});
