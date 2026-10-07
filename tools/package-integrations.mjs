// Prepared toolchains only: run the extension compile step first.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { zip } from './lib/zip.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const version = fs.readFileSync(path.join(root,'VERSION'),'utf8').trim();
const output = path.resolve(process.argv[2] || path.join(root,'dist/integrations'));
if (JSON.parse(fs.readFileSync(path.join(root,'packages/vscode-extension/package.json'))).version !== version) throw Error('Extension version differs from VERSION');
fs.mkdirSync(output,{recursive:true});
execFileSync(process.execPath,[path.join(root,'packages/vscode-extension/scripts/package-vsix.mjs'),path.join(output,`readmd-vscode-${version}.vsix`)],{stdio:'inherit'});
const entries = ['README.md','mcp_config_templates.json'].map(name=>({name,data:fs.readFileSync(path.join(root,'packages/mcp-server',name))}));
entries.push({name:'LICENSE',data:fs.readFileSync(path.join(root,'LICENSE'))});
fs.writeFileSync(path.join(output,`readmd-mcp-server-${version}.zip`),zip(entries));
console.log('Packaged VS Code extension and MCP documentation/configuration kit offline.');
