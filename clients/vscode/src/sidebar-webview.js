'use strict';

// The document and live region survive every update; only data crosses the bridge.
function webviewMain() {
  const api = acquireVsCodeApi();
  const byId = id => document.getElementById(id);
  const form = byId('fields');
  const controls = new Map();
  let model;
  let contextKey;
  const text = (id, value) => { const node = byId(id); if (node.textContent !== value) node.textContent = value; };
  const values = () => Object.fromEntries([...controls].filter(([, entry]) => !entry.label.hidden).map(([name, entry]) => [name, entry.input.value]));
  function validity() {
    for (const { label, input, error } of controls.values()) {
      const invalid = !label.hidden && input.required && !input.value.trim();
      const message = invalid ? '请填写此必填项。' : input.selectedOptions?.[0]?.dataset.invalid ? '请选择当前有效选项。' : '';
      input.setCustomValidity(message);
      error.textContent = message;
      input.setAttribute('aria-invalid', String(!!message));
    }
  }
  window.addEventListener('message', event => {
    if (event.data?.type !== 'update') return;
    model = event.data;
    const changedContext = contextKey !== model.contextKey;
    contextKey = model.contextKey;
    text('scene-title', model.title);
    text('status', model.status);
    text('workspace-note', model.workspace);
    text('file-note', model.file);
    text('authority-note', model.authority);
    text('scope-note', model.scopeNote);
    text('registry-note', model.registryNote);
    text('release-note', model.releaseNote);
    text('descriptor-note', model.descriptorNote);
    text('page-note', model.pageNote);
    text('output', model.detail);
    text('progress-text', model.progress);
    byId('progress').hidden = !model.busy;
    byId('output').setAttribute('aria-busy', String(model.busy));
    for (const [id, show] of Object.entries(model.visible)) byId(id).hidden = !show;
    for (const button of document.querySelectorAll('[data-action]')) {
      button.disabled = button.dataset.action === 'cancel' ? !model.busy
        : model.busy || (button.dataset.action === 'nextPage' && !model.hasMore);
    }
    for (const [name, entry] of controls) {
      entry.label.hidden = !model.controls.some(field => field.name === name);
      entry.input.disabled = entry.label.hidden;
    }
    for (const field of model.controls) {
      let entry = controls.get(field.name);
      if (!entry) {
        const label = document.createElement('label');
        const caption = document.createElement('span');
        const input = document.createElement(field.choices ? 'select' : field.name === 'reason' ? 'textarea' : 'input');
        input.name = field.name; input.id = 'field-' + field.name;
        input.maxLength = 16384;
        label.htmlFor = input.id;
        const error = document.createElement('small'); error.id = input.id + '-error';
        input.setAttribute('aria-describedby', error.id);
        label.append(caption, input, error); form.append(label);
        entry = { label, caption, input, error }; controls.set(field.name, entry);
      }
      const { label, caption, input } = entry;
      label.hidden = false; input.disabled = false;
      caption.textContent = field.label;
      input.required = field.required;
      input.readOnly = model.busy;
      input.setAttribute('aria-disabled', String(model.busy));
      if (field.choices) {
        const signature = JSON.stringify(field.choices);
        if (signature !== entry.choices) {
          const selected = input.value;
          const options = field.choices.map(choice => {
            const option = document.createElement('option'); option.value = choice.value;
            option.textContent = choice.label;
            if (choice.invalid) option.dataset.invalid = 'true';
            return option;
          });
          input.replaceChildren(...options); entry.choices = signature;
          if (!changedContext && document.activeElement === input) input.value = selected;
        }
      }
      // Never reset the caret or overwrite an in-flight edit with an older host echo.
      if ((changedContext || document.activeElement !== input) && input.value !== field.value) input.value = field.value;
    }
    text('execute-label', model.executeLabel);
    if (model.busy && document.activeElement === byId('execute')) byId('cancel').focus();
    byId('execute').disabled = !model.canRun || !model.validChoices;
    validity();
  });
  document.querySelectorAll('[data-action]').forEach(button => button.addEventListener('click', () => api.postMessage({ type: button.dataset.action })));
  form.addEventListener('input', event => {
    if (!model) return;
    if (model.busy) {
      const field = model.controls.find(field => field.name === event.target.name);
      if (field) event.target.value = field.value;
      return;
    }
    validity();
    byId('execute').disabled = !model.canRun || [...controls.values()].some(({ label, input }) => !label.hidden && input.tagName === 'SELECT' && (!input.value || input.selectedOptions[0]?.dataset.invalid));
    api.postMessage({ type: 'edit', fields: values() });
  });
  form.addEventListener('submit', event => {
    event.preventDefault();
    if (!model?.canRun || model.busy) return;
    validity();
    if (form.reportValidity()) api.postMessage({ type: 'run', fields: values() });
  });
  // Read-only selects retain focus while a task runs, but cannot change the run's inputs.
  form.addEventListener('keydown', event => { if (model?.busy && event.target.tagName === 'SELECT' && !['Tab', 'Escape'].includes(event.key)) event.preventDefault(); });
  form.addEventListener('pointerdown', event => { if (model?.busy && event.target.tagName === 'SELECT') event.preventDefault(); });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && model?.busy) { event.preventDefault(); api.postMessage({ type: 'cancel' }); }
  });
  api.postMessage({ type: 'ready' });
}

function html(nonce) {
  const button = (action, label) => `<button type="button" class="secondary" id="${action}" data-action="${action}">${label}</button>`;
  return `<!DOCTYPE html><html lang="zh-CN"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}'"><style nonce="${nonce}">
*{box-sizing:border-box}[hidden]{display:none!important}
html,body{height:100%;margin:0}body{font-family:var(--vscode-font-family);font-size:var(--vscode-font-size);color:var(--vscode-foreground);background:var(--vscode-sideBar-background);display:grid;grid-template-rows:minmax(0,1fr) auto minmax(0,1fr);padding:8px;gap:8px;overflow:hidden}
#form-pane,#output{min-height:0;overflow:auto}#form-pane{padding:4px}#actions{border-block:1px solid var(--vscode-contrastBorder,var(--vscode-panel-border));padding:8px 0}.buttons{display:flex;flex-wrap:wrap;gap:4px}
input,textarea,select{font:inherit;color:var(--vscode-input-foreground);background:var(--vscode-input-background);border:1px solid var(--vscode-input-border,var(--vscode-contrastBorder,transparent));width:100%;padding:6px;margin-top:4px}textarea{min-height:64px;resize:vertical}label{display:block;margin-top:12px}small{display:block;color:var(--vscode-errorForeground)}
input[aria-invalid="true"],textarea[aria-invalid="true"],select[aria-invalid="true"]{border-color:var(--vscode-inputValidation-errorBorder,var(--vscode-errorForeground))}
button{display:inline-flex;align-items:center;gap:4px;font:inherit;border:1px solid var(--vscode-button-border,var(--vscode-contrastBorder,transparent));padding:6px 8px;border-radius:2px;cursor:pointer;white-space:normal;overflow-wrap:anywhere}.primary{color:var(--vscode-button-foreground);background:var(--vscode-button-background)}.primary:hover:not(:disabled){background:var(--vscode-button-hoverBackground)}.secondary{color:var(--vscode-button-secondaryForeground);background:var(--vscode-button-secondaryBackground)}.secondary:hover:not(:disabled){background:var(--vscode-button-secondaryHoverBackground)}button:disabled{color:var(--vscode-disabledForeground);background:var(--vscode-button-secondaryBackground);opacity:1;cursor:not-allowed}
p,pre{white-space:pre-wrap;overflow-wrap:anywhere;margin:4px 0}h2{font-size:1.1em;margin:4px 0}pre,code{font-family:var(--vscode-editor-font-family,monospace);font-size:var(--vscode-editor-font-size,12px);line-height:1.5}:focus-visible{outline:2px solid var(--vscode-focusBorder);outline-offset:1px}#output{padding:4px}#progress{height:3px;overflow:hidden;background:var(--vscode-button-secondaryBackground)}#progress::after{content:'';display:block;width:35%;height:100%;background:var(--vscode-progressBar-background);animation:running 1.2s linear infinite}#progress-text{max-height:3em;overflow:auto}#governance{color:var(--vscode-descriptionForeground)}@keyframes running{from{transform:translateX(-100%)}to{transform:translateX(300%)}}@media(prefers-reduced-motion:reduce){#progress::after{animation:none;width:100%}}
</style></head><body><section id="form-pane" aria-label="场景参数"><h2 id="scene-title"></h2>${button('workspace', '选择工作区')}<p id="workspace-note"></p><section id="finding-section" hidden>${button('pickFile', '选择 FindingView JSON')}<p id="file-note"></p></section><section id="authority-section" hidden>${button('pickAuthority', '选择结果与独立信任根')}${button('clearAuthority', '清除配对')}<p id="authority-note"></p></section><p id="scope-note" hidden></p><section id="registry-section" hidden><p id="registry-note"></p>${button('pickReleasePins', '选择独立发布记录（非信任根）')}<p id="release-note"></p><section id="descriptor-section" hidden>${button('pickDescriptor', '选择外部签名 descriptor')}<p id="descriptor-note"></p></section><section id="page-section" hidden>${button('nextPage', '下一页')}<p id="page-note"></p></section></section><form id="fields" novalidate></form></section><section id="actions" aria-label="执行操作"><p id="status" role="status" aria-live="polite" aria-atomic="true"></p><div id="progress" role="progressbar" aria-label="执行中" hidden></div><p id="progress-text" role="log" aria-live="polite"></p><div class="buttons"><button id="execute" class="primary" type="submit" form="fields" disabled><svg aria-hidden="true" focusable="false" width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M5 3.7v8.6L12 8z"/></svg><span id="execute-label">执行所选场景</span></button>${button('cancel', '取消（Esc）')}${button('refresh', '刷新')}${button('settings', '设置')}</div><p id="governance">提交 ≠ 批准 ≠ Gate PASS；不写入 SCM。</p></section><pre id="output" tabindex="0" role="region" aria-label="执行回执与输出日志"></pre><script nonce="${nonce}">(${webviewMain.toString()})();</script></body></html>`;
}
module.exports = { html, webviewMain };
