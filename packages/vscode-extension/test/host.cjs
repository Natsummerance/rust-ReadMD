// Invoked by VS Code --extensionTestsPath. Uses the real Extension Host and Rust MCP.
const vscode = require('vscode');
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');

exports.run = async function () {
  const root = process.env.READMD_HOST_TEST_ROOT;
  assert.ok(root && path.isAbsolute(root), 'An isolated test root is required');
  const extension = vscode.extensions.all.find(item => item.packageJSON.name === 'readmd-vscode');
  assert.ok(extension, 'Extension is loaded by VS Code');
  await extension.activate();
  const commands = await vscode.commands.getCommands(true);
  for (const command of extension.packageJSON.contributes.commands) assert.ok(commands.includes(command.command), command.command);
  const { ReadMDBridge } = require(path.join(extension.extensionPath, 'out/bridge'));
  const { candidatePaths, probeBinary } = require(path.join(extension.extensionPath, 'out/binaryFinder'));
  const bridge = new ReadMDBridge({ extensionPath: extension.extensionPath, subscriptions: [] });
  const results = [];
  try {
    const installed = candidatePaths(extension.extensionPath).filter(file => path.isAbsolute(file) && fs.existsSync(file));
    assert.ok((await Promise.all(installed.map(probeBinary))).some(Boolean), 'A default installed binary is usable');
    results.push('default installed executable discovery');
    const binary = await bridge.getServerCommand(); assert.ok(fs.existsSync(binary)); results.push('configured executable discovery');
    const tools = (await bridge.callMcpMethod('tools/list')).tools;
    assert.ok(tools.some(tool => tool.name === 'readmd_ai_models'));
    results.push('real MCP handshake and tool discovery');
    const fixed = await bridge.fixMarkdown('# Host acceptance\n\n| A | B |\n| -- | -- |\n| 1 | 2 |\n');
    assert.equal(fixed.ok, true); results.push('native Markdown repair');
    assert.match(await bridge.generateToc('# Host acceptance\n\n## Details\n'), /Details/);
    results.push('native table of contents');
    const source = path.join(root, 'host.md');
    const image = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==', 'base64');
    fs.writeFileSync(path.join(root, 'host-image.png'), image);
    const markdown = '# Host acceptance\n\n![Local](host-image.png)\n\n$x^2$\n\n```mermaid\ngraph LR\n A-->B\n```\n\n---\n\n# Second slide\n';
    fs.writeFileSync(source, markdown);
    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(source));
    await vscode.window.showTextDocument(document);
    await vscode.commands.executeCommand('readmd.preview'); results.push('real preview command');
    await vscode.window.showTextDocument(document);
    await vscode.commands.executeCommand('readmd.openPresentation'); results.push('real presentation command');
    await vscode.window.showTextDocument(document);
    const output = path.join(root, 'host.slides.html');
    await bridge.exportPresentation(markdown, output, 'Host acceptance', 'black', 'slide', true, root);
    assert.match(fs.readFileSync(output, 'utf8'), /Host acceptance/);
    assert.ok(fs.readFileSync(output, 'utf8').includes('data:image/png;base64,' + image.toString('base64')));
    await bridge.exportPresentation('# Replacement', output, 'Replacement', 'black', 'slide', true);
    assert.match(fs.readFileSync(output, 'utf8'), /Replacement/);
    results.push('offline presentation export with local images and explicit replacement');
    await bridge.exportDoc('# Host acceptance\n\nText.', path.join(root, 'host.docx'), 'docx', 'minimal');
    assert.equal(fs.readFileSync(path.join(root, 'host.docx')).subarray(0, 2).toString(), 'PK');
    results.push('native Word export');
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
    fs.writeFileSync(path.join(root, 'host-result.json'), JSON.stringify({ ok: true, vscode: vscode.version, results }, null, 2));
  } finally { bridge.dispose(); }
};
