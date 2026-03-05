'use strict';
// VSCode sets ELECTRON_RUN_AS_NODE=1 which makes Electron skip its API registration.
// Delete it before spawning electron-vite so Electron runs as a proper app.
const { spawn } = require('child_process');
const env = { ...process.env };
delete env.ELECTRON_RUN_AS_NODE;

const child = spawn('npx electron-vite dev', [], { stdio: 'inherit', env, shell: true });
child.on('exit', code => process.exit(code ?? 0));
