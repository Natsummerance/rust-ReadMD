import * as vscode from 'vscode';
import { ReadMDBridge } from './bridge';

/** Native VS Code ghost text + Tab/undo, using the same bounded Rust endpoint.
 * Opt-in and trusted workspaces only. Never uses clipboard or other files. */
export function registerCompletion(context: vscode.ExtensionContext, bridge: ReadMDBridge): void {
  const cache = new Map<string,{text:string;time:number}>();
  let generation = 0;
  context.subscriptions.push(vscode.workspace.onDidChangeConfiguration(e => {
    if (e.affectsConfiguration('readmd')) { generation++; cache.clear(); }
  }));
  context.subscriptions.push(vscode.languages.registerInlineCompletionItemProvider(
    {language:'markdown',scheme:'file'}, {
      async provideInlineCompletionItems(document,position,_context,token) {
        if (!vscode.workspace.isTrusted || !vscode.workspace.getConfiguration('readmd').get<boolean>('aiAutocomplete',false)
          || token.isCancellationRequested || document.getText().length > 2*1024*1024) return [];
        const editor = vscode.window.activeTextEditor;
        if (!editor || editor.document !== document || editor.selections.length !== 1 || !editor.selection.isEmpty
          || /^\s*\|/.test(document.lineAt(position).text)) return [];
        const version=document.version, serial=++generation;
        const requested=vscode.workspace.getConfiguration('readmd').get<string>('aiAutocompleteModel','');
        const model=typeof requested==='string'?requested.trim():'';
        const text=document.getText(), offset=document.offsetAt(position);
        const prefix=Array.from(text.slice(Math.max(0,offset-6000),offset).replace(/^[\uDC00-\uDFFF]/,'')).slice(-3000).join('');
        const suffix=Array.from(text.slice(offset,offset+1500).replace(/[\uD800-\uDBFF]$/,'')).slice(0,750).join('');
        if (Array.from(prefix.trim()).length<4 || model.length>256) return [];
        const key=JSON.stringify([model,prefix,suffix]);
        const immediate=cache.get(key);
        await new Promise<void>(resolve=>{
          let cancel:vscode.Disposable|undefined;
          const timer=setTimeout(()=>{cancel?.dispose();resolve();},immediate && Date.now()-immediate.time<60000?0:250);
          cancel=token.onCancellationRequested(()=>{clearTimeout(timer);cancel?.dispose();resolve();});
          if(token.isCancellationRequested){clearTimeout(timer);cancel.dispose();resolve();}
        });
        const valid=()=>!token.isCancellationRequested && serial===generation && document.version===version
          && vscode.window.activeTextEditor===editor && editor.selection.isEmpty && editor.selection.active.isEqual(position);
        if (!valid()) return [];
        try {
          const cached=cache.get(key);
          let suggestion=cached && Date.now()-cached.time<60000?cached.text:undefined;
          if (suggestion===undefined) {
            const result=await bridge.callMcpTool('readmd_ai_complete',{prefix,suffix,...(model?{model}:{})},token);
            if (!valid()) return [];
            const generated:string=typeof result.completion==='string'?result.completion:'';
            suggestion=generated;
            cache.set(key,{text:generated,time:Date.now()});while(cache.size>24)cache.delete(cache.keys().next().value!);
          }
          if (!suggestion || !valid()) return [];
          return [new vscode.InlineCompletionItem(suggestion,new vscode.Range(position,position))];
        } catch { return []; }
      }
    }));
}
