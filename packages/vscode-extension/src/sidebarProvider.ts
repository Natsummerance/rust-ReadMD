import * as vscode from 'vscode';
import { l10n } from './localization';
export interface SkillEntry { name?: string; uri: string; description?: string; }

const actions: Record<string, [string, string, string, string]> = {
  preview: ['readmd.preview','toolPreview','Preview','book'], ai: ['readmd.openAiWorkbench','toolAi','AI Workbench','sparkle'],
  fix: ['readmd.fixCurrentDocument','toolFix','Repair Markdown','wrench'], toc: ['readmd.insertToc','toolToc','Table of Contents','list-tree'],
  code: ['readmd.insertCodeChunk','toolCode','Code Block','code'], diagram: ['readmd.insertDiagram','toolDiagram','Diagram','graph'],
  metadata: ['readmd.insertFrontmatter','toolMetadata','Document Metadata','settings-gear'], slide: ['readmd.insertSlide','toolSlide','Slide Break','split-horizontal'],
  import: ['readmd.insertDocImport','toolImport','Document Reference','references'], run: ['readmd.runCodeChunk','toolRun','Run Code Block…','play'],
  convert: ['readmd.convertAnyFilePrompt','toolConvert','Convert Local File…','file-symlink-file'], web: ['readmd.fetchWebToMarkdown','toolWeb','Extract Web Page…','globe'],
  flatten: ['readmd.processImports','toolFlatten','Expand Document References','references'], bib: ['readmd.parseBibtex','toolBib','Read BibTeX…','references'],
  export: ['readmd.exportDocument','toolExport','Export Document…','export'], presentation: ['readmd.openPresentation','toolPresentation','Present Slides','screen-full'],
  exportSlides: ['readmd.exportPresentation','toolExportSlides','Export Slides…','file-media'], latex: ['readmd.convertToLatex','toolLatex','Generate LaTeX','file-code'],
  mcp: ['readmd.setupMcpServer','toolMcp','Configure MCP Client…','plug'],
};
function action(id: string): ToolboxItem {
  const [command,key,fallback,icon] = actions[id]; const label = l10n(key,fallback);
  return new ToolboxItem(label,vscode.TreeItemCollapsibleState.None,'cmd',[],{command,title:label},icon);
}
function group(key: string,fallback: string,context: string,ids: string[],icon: string): ToolboxItem {
  return new ToolboxItem(l10n(key,fallback),vscode.TreeItemCollapsibleState.Collapsed,context,ids.map(action),undefined,icon);
}
export class ReadMDToolboxProvider implements vscode.TreeDataProvider<ToolboxItem> {
  private readonly changes = new vscode.EventEmitter<ToolboxItem | undefined | null | void>();
  readonly onDidChangeTreeData = this.changes.event;
  constructor(private readonly listSkills?: () => Promise<SkillEntry[]>) {}
  refresh(): void { this.changes.fire(); }
  dispose(): void { this.changes.dispose(); }
  getTreeItem(element: ToolboxItem): vscode.TreeItem { return element; }
  async getChildren(element?: ToolboxItem): Promise<ToolboxItem[]> {
    if (!element) return [action('preview'),action('ai'),
      group('toolEditing','Edit & Insert','group_fix',['fix','toc','code','diagram','metadata','import','slide','run'],'edit'),
      group('toolConversion','Convert & Import','group_convert',['convert','web','flatten','bib'],'references'),
      group('toolExporting','Export & Present','group_export',['export','presentation','exportSlides','latex'],'export'),
      group('toolSkills','ReadMD Skills','group_skills',[],'library'), group('toolIntegration','Integrations','group_mcp',['mcp'],'plug')];
    if (element.contextValue !== 'group_skills') return element.children;
    if (!this.listSkills) return [new ToolboxItem(l10n('skillsUnavailable','Skills unavailable; ReadMD Core is disconnected'),vscode.TreeItemCollapsibleState.None,'skill_unavailable')];
    try { return buildSkillItems(await this.listSkills()); }
    catch { return [new ToolboxItem(l10n('skillsListFailed','Could not load Skills'),vscode.TreeItemCollapsibleState.None,'skill_error')]; }
  }
}
export function buildSkillItems(skills: SkillEntry[]): ToolboxItem[] {
  if (!skills.length) return [new ToolboxItem(l10n('skillsEmpty','No Skills available'),vscode.TreeItemCollapsibleState.None,'skill_empty')];
  return skills.map(skill => new ToolboxItem(skill.name || skill.uri,vscode.TreeItemCollapsibleState.None,'skill',[],
    {command:'readmd.openSkillByUri',title:l10n('skillOpen','Open Skill'),arguments:[skill.uri]},'library',skill.description));
}
export class ToolboxItem extends vscode.TreeItem {
  constructor(public readonly label: string,public readonly collapsibleState: vscode.TreeItemCollapsibleState,
      public readonly contextValue: string,public readonly children: ToolboxItem[] = [],public readonly command?: vscode.Command,
      public readonly iconName?: string,public readonly itemDescription?: string) {
    super(label,collapsibleState);
    if (itemDescription) this.description = itemDescription;
    if (iconName) this.iconPath = new vscode.ThemeIcon(iconName);
  }
}
