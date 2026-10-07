import * as vscode from 'vscode';
import * as path from 'path';
import * as cp from 'child_process';
import { StringDecoder } from 'string_decoder';
import { findReadmdBinary } from './binaryFinder';

export interface FixResult {
  ok: boolean;
  repaired_content: string;
  fixes_count: number;
  fixes_details: string[];
  stats: Record<string, number>;
}

export interface WebResult {
  ok: boolean;
  title: string;
  markdown: string;
  url: string;
  engine?: string;
  warnings?: string[];
}

export class ReadMDBridge {
  private extensionPath: string;
  private proc?: cp.ChildProcessWithoutNullStreams;
  private starting?: Promise<void>;
  private nextId = 1;
  private buffer = '';
  private decoder = new StringDecoder('utf8');
  private pending = new Map<number, {
    resolve: (value: any) => void;
    reject: (reason?: any) => void;
    idleTimer: NodeJS.Timeout;
    maxTimer: NodeJS.Timeout;
    resetIdle: () => void;
    onProgress?: (message: string) => void;
  }>();
  private disposed = false;
  private configurationGeneration = 0;
  private procSpawned = false;
  private everConnected = false;
  private disconnectedListeners = new Set<() => void>();
  private readyListeners = new Set<() => void>();

  constructor(context: vscode.ExtensionContext) {
    this.extensionPath = context.extensionPath;
    const changed = vscode.workspace.onDidChangeConfiguration?.(event => {
      if (!event.affectsConfiguration('readmd.executablePath')) return;
      this.configurationGeneration++;
      this.resolvedBinary = undefined;
      this.failProcess(new Error('core_configuration_changed'));
    });
    if (changed) context.subscriptions.push(changed);
  }

  private resolvedBinary?: string;

  /** The ReadMD executable; `readmd --mcp` is the MCP server (no Python). */
  public async getServerCommand(): Promise<string> {
    if (!this.resolvedBinary) {
      const generation = this.configurationGeneration;
      const binary = await findReadmdBinary(this.extensionPath);
      if (generation !== this.configurationGeneration) throw new Error('core_configuration_changed');
      this.resolvedBinary = binary;
    }
    return this.resolvedBinary;
  }

  public onDisconnected(listener: () => void): vscode.Disposable {
    this.disconnectedListeners.add(listener);
    return { dispose: () => this.disconnectedListeners.delete(listener) };
  }

  public onReady(listener: () => void): vscode.Disposable {
    this.readyListeners.add(listener);
    return { dispose: () => this.readyListeners.delete(listener) };
  }

  private fireDisconnected(): void {
    for (const listener of [...this.disconnectedListeners]) listener();
  }

  private fireReady(): void {
    for (const listener of [...this.readyListeners]) listener();
  }

  private async ensureProcess(): Promise<void> {
    if (this.disposed) throw new Error('core_closed');
    if (this.starting) return this.starting;
    if (this.proc && this.procSpawned && !this.proc.killed) return;
    this.starting = (async () => {
      const binary = await this.getServerCommand();
      if (this.disposed) throw new Error('core_closed');
      const proc = cp.spawn(binary, ['--mcp'], {
        stdio: ['pipe', 'pipe', 'pipe'],
        windowsHide: true,
      });
      this.proc = proc;
      this.procSpawned = false;
      this.decoder = new StringDecoder('utf8');
      proc.stdout.on('data', chunk => {
        if (this.proc !== proc) return;
        const text = typeof chunk === 'string' ? chunk : this.decoder.write(chunk);
        this.consumeOutput(text);
      });
      proc.stderr.on('data', chunk => { /* protocol responses stay on stdout */ void chunk; });
      proc.stdin.on?.('error', () => {
        if (this.proc === proc) this.failProcess(new Error('core_not_connected'));
      });
      proc.on('error', err => {
        // A binary that vanished (uninstall/upgrade) is looked up again next time.
        this.resolvedBinary = undefined;
        if (this.proc === proc) this.failProcess(err);
      });
      proc.on('close', code => {
        if (this.proc === proc) this.failProcess(new Error('core_process_exit'));
      });
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => {
          if (this.proc === proc) this.failProcess(new Error('core_start_timeout'));
          reject(new Error('core_start_timeout'));
        }, 10000);
        proc.once('spawn', () => {
          if (this.proc !== proc) return;
          this.procSpawned = true;
          clearTimeout(timer);
          resolve();
        });
        proc.once('error', err => { clearTimeout(timer); reject(err); });
        proc.once('close', () => { clearTimeout(timer); reject(new Error('core_process_exit')); });
      });
      // Ready means the protocol is usable, not merely that an OS process exists.
      const hello = await this.callMcpMethodInternal('initialize', {
        protocolVersion: '2025-11-25', capabilities: {},
        clientInfo: { name: 'readmd-vscode', version: '0.0.4' },
      }, undefined, undefined, true);
      if (!['2025-11-25', '2025-06-18', '2025-03-26', '2024-11-05'].includes(hello?.protocolVersion)) {
        this.failProcess(new Error('core_protocol_unsupported'));
        throw new Error('core_protocol_unsupported');
      }
      proc.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
      this.everConnected = true;
      this.fireReady();
    })().catch(error => {
      this.failProcess(error instanceof Error ? error : new Error('core_not_connected'));
      throw error;
    }).finally(() => { this.starting = undefined; });
    return this.starting;
  }

  private consumeOutput(chunk: string): void {
    this.buffer += chunk;
    if (Buffer.byteLength(this.buffer, 'utf8') > 32 * 1024 * 1024) {
      this.failProcess(new Error('core_response_too_large'));
      return;
    }
    let idx = this.buffer.indexOf('\n');
    while (idx >= 0) {
      const line = this.buffer.slice(0, idx).trim();
      this.buffer = this.buffer.slice(idx + 1);
      if (line) {
        try {
          const response = JSON.parse(line);
          if (response.method === 'notifications/progress') {
            const token = Number(response.params?.progressToken);
            const waiter = this.pending.get(token);
            if (waiter) {
              waiter.resetIdle();
              const message = String(response.params?.message ?? '');
              if (waiter.onProgress && message) waiter.onProgress(message);
            }
          } else {
            const id = typeof response.id === 'number' ? response.id : NaN;
            const waiter = this.pending.get(id);
            if (waiter) {
              this.pending.delete(id);
              clearTimeout(waiter.idleTimer);
              clearTimeout(waiter.maxTimer);
              if (response.error) waiter.reject(new Error(String(response.error.code || 'mcp_request_failed')));
              else waiter.resolve(response.result);
            }
          }
        } catch { /* ignore partial/non-protocol output */ }
      }
      idx = this.buffer.indexOf('\n');
    }
  }

  private failProcess(error: Error): void {
    const proc = this.proc;
    const wasConnected = !!proc && this.procSpawned && this.everConnected && !this.disposed;
    this.proc = undefined;
    this.procSpawned = false;
    proc?.kill();
    for (const waiter of this.pending.values()) {
      clearTimeout(waiter.idleTimer);
      clearTimeout(waiter.maxTimer);
      waiter.reject(error);
    }
    this.pending.clear();
    this.buffer = '';
    this.decoder.end();
    if (wasConnected) this.fireDisconnected();
  }

  /**
   * 调用 MCP 工具调度器执行核心能力。
   */
  public async callMcpTool(name: string, args: Record<string, any>): Promise<any> {
    const result = await this.callMcpMethod('tools/call', { name, arguments: args });
    return this.unwrapToolResult(result);
  }

  private unwrapToolResult(result: any): any {
    if (result?.isError) {
      let parsed = result.structuredContent;
      if (!parsed) { try { parsed = JSON.parse(result.content?.[0]?.text || '{}'); } catch {} }
      throw new Error(String(parsed?.error_code || 'mcp_tool_failed'));
    }
    if (result?.structuredContent !== undefined) return result.structuredContent;
    const text = result?.content?.[0]?.text;
    try { return text ? JSON.parse(text) : result; } catch { return text || result; }
  }

  /** Call a persistent MCP JSON-RPC method (resources/prompts included). */
  public async callMcpMethod(method: string, params: Record<string, any> = {}): Promise<any> {
    return this.callMcpMethodInternal(method, params);
  }

  private async callMcpMethodInternal(method: string, params: Record<string, any>,
      onProgress?: (message: string) => void, token?: vscode.CancellationToken, duringHandshake = false): Promise<any> {
    if (token?.isCancellationRequested) throw new Error('ai_cancelled');
    if (!duringHandshake) await this.ensureProcess();
    if (token?.isCancellationRequested) throw new Error('ai_cancelled');
    const proc = this.proc;
    if (!proc || !proc.stdin.writable) throw new Error('core_not_connected');
    const id = this.nextId++;
    if (onProgress || method === 'tools/call') {
      params = { ...params, _meta: { ...(params._meta || {}), progressToken: id } };
    }
    const request = { jsonrpc: '2.0', id, method, params };
    const payload = JSON.stringify(request) + '\n';
    if (Buffer.byteLength(payload, 'utf8') > 32 * 1024 * 1024) throw new Error('core_request_too_large');
    return new Promise((resolve, reject) => {
      const IDLE_TIMEOUT_MS = 45000;
      const MAX_TIMEOUT_MS = 600000; // 10 minutes
      let idleTimer!: NodeJS.Timeout;
      let maxTimer!: NodeJS.Timeout;
      let cancelDisposable: vscode.Disposable | undefined;
      let settled = false;

      const sendCancelNotice = () => {
        if (proc.stdin.writable) {
          try {
            proc.stdin.write(JSON.stringify({
              jsonrpc: '2.0', method: 'notifications/cancelled', params: { requestId: id },
            }) + '\n');
          } catch { /* process may already be gone */ }
        }
      };

      const settle = (ok: boolean, value: any) => {
        if (settled) return;
        settled = true;
        this.pending.delete(id);
        clearTimeout(idleTimer);
        clearTimeout(maxTimer);
        cancelDisposable?.dispose();
        (ok ? resolve : reject)(value);
      };

      const resetIdle = () => {
        clearTimeout(idleTimer);
        idleTimer = setTimeout(() => {
          sendCancelNotice();
          settle(false, new Error('core_operation_timeout'));
        }, IDLE_TIMEOUT_MS);
      };

      resetIdle();
      maxTimer = setTimeout(() => {
        sendCancelNotice();
        settle(false, new Error('core_operation_timeout'));
      }, MAX_TIMEOUT_MS);

      this.pending.set(id, {
        resolve: value => settle(true, value),
        reject: value => settle(false, value),
        idleTimer,
        maxTimer,
        resetIdle,
        onProgress,
      });

      if (token) {
        cancelDisposable = token.onCancellationRequested(() => {
          sendCancelNotice();
          settle(false, new Error('ai_cancelled'));
        });
      }
      try {
        proc.stdin.write(payload, error => {
          if (error) settle(false, new Error('core_not_connected'));
        });
      } catch { settle(false, new Error('core_not_connected')); }
    });
  }

  /** Tool call with progress messages (these are status, not AI text deltas).
   * an optional CancellationToken cancels via notifications/cancelled. */
  public async callMcpToolStreaming(name: string, args: Record<string, any>,
      onChunk: (chunk: string) => void, token?: vscode.CancellationToken): Promise<any> {
    const result = await this.callMcpMethodInternal('tools/call', { name, arguments: args }, onChunk, token);
    return this.unwrapToolResult(result);
  }

  public async listSkills(): Promise<any[]> {
    const resources = await this.collectPages('resources/list', 'resources');
    return resources.filter(item => typeof item?.uri === 'string' && item.uri.startsWith('readmd://skills/'));
  }

  private async collectPages(method: string, key: string): Promise<any[]> {
    const items: any[] = [];
    const seen = new Set<string>();
    let cursor: string | undefined;
    do {
      const page = await this.callMcpMethod(method, cursor ? { cursor } : {});
      if (Array.isArray(page?.[key])) items.push(...page[key]);
      cursor = typeof page?.nextCursor === 'string' && page.nextCursor ? page.nextCursor : undefined;
      if (cursor && seen.has(cursor)) throw new Error('core_invalid_pagination');
      if (cursor) seen.add(cursor);
      if (seen.size > 100) throw new Error('core_invalid_pagination');
    } while (cursor);
    return items;
  }

  /** Return the Core's current Skill-backed prompt descriptors. */
  public async listPrompts(): Promise<any[]> {
    return this.collectPages('prompts/list', 'prompts');
  }

  public async readSkill(uri: string): Promise<string> {
    const result = await this.callMcpMethod('resources/read', { uri });
    return result?.contents?.[0]?.text || '';
  }

  public async getPrompt(workflowId: string, markdownContent: string, request = ''): Promise<any> {
    return this.callMcpMethod('prompts/get', { name: workflowId, arguments: { markdown_content: markdownContent, request } });
  }

  public async listProviders(): Promise<any[]> {
    const result = await this.callMcpTool('readmd_ai_providers', {});
    return result?.providers || [];
  }

  public async listModels(provider: string, credentialId?: string): Promise<string[]> {
    const result = await this.callMcpTool('readmd_ai_models', { provider, ...(credentialId ? { credential_id: credentialId } : {}), confirm: true });
    return Array.isArray(result?.models) ? result.models.map((model: any) => typeof model === 'string' ? model : model?.id).filter((id: unknown): id is string => typeof id === 'string' && !!id) : [];
  }

  public async exportPresets(): Promise<string[]> {
    const catalog = await this.callMcpTool('readmd_export_presets', {});
    return [...new Set([...Object.keys(catalog?.presets || {}), ...Object.keys(catalog?.custom || {})])];
  }

  public async aiChat(args: Record<string, any>): Promise<any> {
    return this.callMcpTool('readmd_ai_chat', args);
  }

  public aiChatStreaming(args: Record<string, any>, onChunk: (chunk: string) => void,
      token?: vscode.CancellationToken): Promise<any> {
    return this.callMcpToolStreaming('readmd_ai_chat', args, onChunk, token);
  }

  public dispose(): void {
    this.disposed = true;
    for (const waiter of this.pending.values()) {
      clearTimeout(waiter.idleTimer);
      clearTimeout(waiter.maxTimer);
      waiter.reject(new Error('core_closed'));
    }
    this.pending.clear(); this.proc?.kill(); this.proc = undefined;
  }

  /**
   * 一键自愈当前 Markdown 文本。
   */
  public async fixMarkdown(content: string): Promise<FixResult> {
    return this.callMcpTool('readmd_fix_markdown', { content });
  }

  /**
   * 本地文件转 Markdown。
   */
  public async convertFile(filePath: string): Promise<string> {
    return this.callMcpTool('readmd_convert_to_markdown', { file_path: filePath });
  }

  /**
   * 网页 URL 抓取并转为 Markdown。
   */
  public async fetchWeb(url: string): Promise<WebResult> {
    return this.callMcpTool('readmd_web_to_markdown', { url, confirm: true });
  }

  /**
   * 导出文档。
   */
  public async exportDoc(markdown: string, outputPath: string, format: string, preset: string, title?: string, baseDir?: string, overwrite = false): Promise<any> {
    return this.callMcpTool('readmd_export_document', {
      markdown_content: markdown,
      output_path: outputPath,
      output_format: format,
      style_preset: preset,
      title: title || 'ReadMD Document',
      ...(baseDir ? { base_dir: baseDir } : {}), overwrite,
      confirm: true,
    });
  }

  /**
   * Markdown 转学术 LaTeX。
   */
  public async mdToLatex(markdown: string, title?: string): Promise<string> {
    return this.callMcpTool('readmd_md_to_latex', {
      markdown_content: markdown,
      doc_title: title || 'ReadMD Paper',
    });
  }

  /**
   * LaTeX 转 Markdown。
   */
  public async latexToMd(latex: string): Promise<string> {
    return this.callMcpTool('readmd_latex_to_md', { latex_content: latex });
  }

  /**
   * 解析并展平 @import 模块化导入。
   */
  public async processImports(content: string, baseDir: string): Promise<string> {
    return this.callMcpTool('readmd_process_imports', {
      markdown_content: content,
      base_dir: baseDir,
    });
  }

  /**
   * 生成 [TOC] 目录树。
   */
  public async generateToc(content: string, depthFrom = 1, depthTo = 6, ordered = false): Promise<string> {
    return this.callMcpTool('readmd_generate_toc', {
      markdown_content: content,
      depth_from: depthFrom,
      depth_to: depthTo,
      ordered_list: ordered,
    });
  }

  /**
   * 导出 Reveal.js 演说幻灯片 HTML。
   */
  public async exportPresentation(content: string, outputPath: string, title?: string, theme = 'black', transition = 'slide', overwrite = false, baseDir?: string): Promise<any> {
    return this.callMcpTool('readmd_export_presentation', {
      markdown_content: content,
      output_path: outputPath,
      title: title || 'ReadMD Presentation',
      theme,
      transition,
      overwrite,
      base_dir: baseDir || path.dirname(outputPath),
      confirm: true,
    });
  }

  /**
   * 导出标准 EPUB 3.0 电子书。
   */
  public async exportEpub(content: string, outputPath: string, title?: string, author?: string, language = 'en', baseDir?: string, overwrite = false): Promise<any> {
    return this.callMcpTool('readmd_export_epub', {
      markdown_content: content,
      output_path: outputPath,
      title: title || 'ReadMD Book',
      author: author || 'ReadMD Author',
      language,
      ...(baseDir ? { base_dir: baseDir } : {}), overwrite,
      confirm: true,
    });
  }

  /**
   * 安全执行代码块。
   */
  public async runCodeChunk(code: string, language = 'python', capturePlot = true): Promise<any> {
    return this.callMcpTool('readmd_run_code_chunk', {
      code,
      language,
      capture_plot: capturePlot,
      confirm: true,
    });
  }

  /**
   * 解析 BibTeX 文件。
   */
  public async parseBibtex(bibPath: string): Promise<any> {
    return this.callMcpTool('readmd_parse_bibtex', { bib_file_path: bibPath });
  }
}
