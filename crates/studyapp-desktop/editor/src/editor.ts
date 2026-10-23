import { ChangeSet, Compartment, EditorSelection, EditorState, Extension, Text, Transaction } from "@codemirror/state";
import { drawSelection, EditorView, keymap, placeholder, ViewUpdate } from "@codemirror/view";
import {
  addCursorAbove, addCursorBelow, copyLineDown, copyLineUp, cursorMatchingBracket,
  deleteGroupBackward, deleteGroupForward, deleteLine, history, indentLess, indentMore,
  indentSelection, insertBlankLine, moveLineDown, moveLineUp, redoSelection, selectAll,
  selectLine, selectParentSyntax, toggleBlockComment, toggleComment, toggleTabFocusMode, undoSelection
} from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxHighlighting } from "@codemirror/language";
import { codeHighlighting, fencedLanguage } from "./languages";
import { renderedMarkdown } from "./decorations";
import { formatCommand } from "./formatting";
import { documentBudget, documentLimits, MAX_DOCUMENT_BYTES, MAX_TRANSACTION_PATCHES, stringBytes } from "./limits";
import { incompleteScalars, scalarText, splitsScalar } from "./unicode";
import { eventBinding, resolveBindings, typingKeymap } from "./shortcuts";

interface Load { generation: number; text: string; readonly: boolean; }
interface TestAction { type: string; request?: string | number; from?: number; to?: number; text?: string; anchor?: number; head?: number; name?: string; edits?: number; }
interface Bridge {
  load(value: Load): void;
  setReadonly(value: boolean): void;
  setSource(value: boolean): void;
  setShortcuts(bindings: Record<string, string>): void;
  command(name: string): boolean;
  flush(value: { request: number }): Promise<void>;
  test?: (value: TestAction) => unknown;
}
declare global {
  interface Window {
    ipc?: { postMessage(message: string): void };
    studyappEditor: Bridge;
    __STUDYAPP_EDITOR_TEST__?: boolean;
  }
}

const readOnlyMode = new Compartment();
const renderingMode = new Compartment();
const shortcutsMode = new Compartment();
let bindings = resolveBindings({});
let generation = 0;
let revision = 0;
let source = false;
let loaded = false;
let loadEpoch = 0;
let pendingFlush = false;
let unpublished: { base: EditorState; changes: ChangeSet } | undefined;
const status = document.querySelector<HTMLElement>("#editor-status")!;
const toolbar = document.querySelector<HTMLElement>("#toolbar")!;
const sourceButton = document.querySelector<HTMLButtonElement>("#source-toggle")!;

function post(message: object): void {
  try { window.ipc?.postMessage(JSON.stringify(message)); }
  catch {
    status.textContent = "无法连接笔记存储，请保留当前内容";
    status.classList.add("error");
  }
}
function error(message: string): void {
  status.textContent = message;
  status.classList.add("error");
  post({ type: "error", generation, revision, message });
}
function warning(message: string): void {
  status.textContent = message;
  status.classList.add("warning");
}
function clearWarning(): void {
  if (status.classList.contains("warning")) {
    status.classList.remove("warning");
    status.textContent = "";
  }
}
function updateToolbar(): void {
  for (const button of toolbar.querySelectorAll<HTMLButtonElement>("button[data-command]")) {
    button.disabled = button.dataset.command !== "source" && (view.state.readOnly || pendingFlush);
    const binding = bindings[`editor.${button.dataset.command}`];
    button.title = `${button.getAttribute("aria-label") ?? ""}${binding ? ` · ${binding.replaceAll(";", " / ")}` : ""}`;
  }
  sourceButton.setAttribute("aria-pressed", String(source));
  if (!status.classList.contains("error") && !status.classList.contains("warning")) status.textContent = pendingFlush ? "正在确认输入…" : view.state.readOnly ? "只读" : "";
}
function readonlyExtensions(value: boolean): Extension {
  return [EditorState.readOnly.of(value), EditorView.editable.of(!value)];
}
function publishPatchSet(base: Text, changes: ChangeSet): void {
  const patches: { fromLine: number; fromColumn: number; toLine: number; toColumn: number; text: string }[] = [];
  changes.iterChanges((from, to, _newFrom, _newTo, inserted) => {
    const first = base.lineAt(from), last = base.lineAt(to);
    patches.push({
      fromLine: first.number, fromColumn: from - first.from + 1,
      toLine: last.number, toColumn: to - last.from + 1, text: inserted.toString()
    });
  });
  if (!patches.length) return;
  const baseRevision = revision++;
  post({ type: "change", generation, baseRevision, revision, changes: patches });
}
function publishBatches(base: EditorState, changes: ChangeSet): void {
  const budget = base.field(documentBudget);
  const shrinking: { from: number; to: number; insert: Text }[] = [];
  const growing: typeof shrinking = [];
  changes.iterChanges((from, to, _newFrom, _newTo, insert) => {
    const firstLine = base.doc.lineAt(from).number - 1;
    const lastLine = base.doc.lineAt(to).number - 1;
    let removedBytes = stringBytes(base.doc.sliceString(from, to));
    for (let line = firstLine; line < lastLine; line++) removedBytes += budget.newlineWidths[line] - 1;
    const insertedBytes = stringBytes(insert.toString()) + (insert.lines - 1) * (budget.preferredWidth - 1);
    (insertedBytes < removedBytes ? shrinking : growing).push({ from, to, insert });
  });
  let applied = ChangeSet.empty(base.doc.length);
  let current = base.doc;
  // Keep original patches and untouched raw EOLs. Shrinking first ensures an
  // intermediate native revision cannot exceed either endpoint's byte budget.
  // Mapping each batch through prior batches keeps its line/column base exact.
  for (const group of [shrinking, growing]) {
    for (let offset = 0; offset < group.length; offset += MAX_TRANSACTION_PATCHES) {
      const batch = ChangeSet.of(group.slice(offset, offset + MAX_TRANSACTION_PATCHES), base.doc.length).map(applied);
      publishPatchSet(current, batch);
      current = batch.apply(current);
      applied = applied.compose(batch);
    }
  }
}
function publishChanges(update: ViewUpdate): void {
  if (!loaded) return;
  for (const transaction of update.transactions) {
    if (!transaction.docChanged) continue;
    // Windows can deliver an astral character as separate UTF-16 key events.
    // Compose those transient edits before crossing Rust's scalar-string IPC
    // boundary; never emit an unpaired surrogate or a split-scalar position.
    const pending = unpublished
      ? { base: unpublished.base, changes: unpublished.changes.compose(transaction.changes) }
      : { base: transaction.startState, changes: transaction.changes };
    unpublished = pending;
    let patches = 0;
    let valid = true;
    pending.changes.iterChanges((from, to, _newFrom, _newTo, inserted) => {
      patches++;
      valid &&= scalarText(inserted.toString()) && !splitsScalar(pending.base.doc, from) && !splitsScalar(pending.base.doc, to);
    });
    if (!valid) continue;
    unpublished = undefined;
    if (patches > MAX_TRANSACTION_PATCHES) publishBatches(pending.base, pending.changes);
    else publishPatchSet(pending.base.doc, pending.changes);
  }
}
const editorCommands: Readonly<Record<string, (view: EditorView) => boolean>> = Object.freeze({
  "undo-selection": undoSelection,
  "redo-selection": redoSelection,
  "move-line-up": moveLineUp,
  "move-line-down": moveLineDown,
  "copy-line-up": copyLineUp,
  "copy-line-down": copyLineDown,
  "add-cursor-above": addCursorAbove,
  "add-cursor-below": addCursorBelow,
  "insert-blank-line": insertBlankLine,
  "select-line": selectLine,
  "select-parent-syntax": selectParentSyntax,
  "indent-less": indentLess,
  "indent-more": indentMore,
  "indent-selection": indentSelection,
  "delete-line": deleteLine,
  "matching-bracket": cursorMatchingBracket,
  "toggle-comment": toggleComment,
  "toggle-block-comment": toggleBlockComment,
  "toggle-tab-focus": toggleTabFocusMode,
  "select-all": selectAll,
  "delete-group-backward": deleteGroupBackward,
  "delete-group-forward": deleteGroupForward
});
function command(name: string): boolean {
  if (typeof name !== "string" || !loaded || view.composing || pendingFlush) return false;
  name = name.startsWith("editor.") ? name.slice(7) : name;
  if (name === "source") { setSource(!source); return true; }
  if (name === "save") { post({ type: "save", generation, revision }); return true; }
  if (view.state.readOnly) return false;
  const result = Object.hasOwn(editorCommands, name) ? editorCommands[name](view) : formatCommand(view, name);
  if (result) view.focus();
  return result;
}
function shortcutLookup(values: Record<string, string>): Map<string, string> {
  const lookup = new Map<string, string>();
  for (const [action, binding] of Object.entries(values)) {
    for (const alternative of binding.split(";").filter(Boolean)) lookup.set(alternative, action);
  }
  return lookup;
}
let activeShortcutLookup = shortcutLookup(bindings);
const knownShortcutKeys = new Set(activeShortcutLookup.keys());
function handleShortcut(event: KeyboardEvent, lookup: Map<string, string>): boolean {
  if (event.defaultPrevented || view.composing) return false;
  const binding = eventBinding(event);
  const action = binding && lookup.get(binding);
  if (!action) {
    // Chromium emits beforeinput(historyUndo/historyRedo) for its own Ctrl+Z/Y
    // defaults. Consuming retired bindings prevents that hidden fallback.
    if (binding && knownShortcutKeys.has(binding)) {
      event.preventDefault();
      return true;
    }
    return false;
  }
  // Once matched, consume even a readonly/no-op command, so browser commands
  // cannot activate and a physical keydown cannot save or route twice.
  event.preventDefault();
  if (event.repeat || pendingFlush) return true;
  if (action.startsWith("app.")) post({ type: "shortcut", action, generation });
  else command(action);
  return true;
}
function shortcutExtensions(values: Record<string, string>): Extension {
  const lookup = shortcutLookup(values);
  return EditorView.domEventHandlers({ keydown: event => handleShortcut(event, lookup) });
}
function setShortcuts(values: Record<string, string>): void {
  let resolved: Record<string, string>;
  try { resolved = resolveBindings(values); }
  catch (cause) {
    // Invalid configuration never changes live bindings or poisons note saving.
    warning(`快捷键配置无效：${cause instanceof Error ? cause.message : String(cause)}`);
    return;
  }
  view.dispatch({ effects: shortcutsMode.reconfigure(shortcutExtensions(resolved)) });
  bindings = resolved;
  activeShortcutLookup = shortcutLookup(resolved);
  for (const binding of activeShortcutLookup.keys()) knownShortcutKeys.add(binding);
  clearWarning();
  updateToolbar();
}
function extensions(readonly: boolean, rawText = ""): Extension[] {
  return [
    readOnlyMode.of(readonlyExtensions(readonly)), renderingMode.of(source ? [] : renderedMarkdown),
    documentLimits(rawText, warning, clearWarning),
    markdown({ base: markdownLanguage, codeLanguages: fencedLanguage, completeHTMLTags: false, pasteURLAsLink: false }),
    syntaxHighlighting(codeHighlighting), history(), drawSelection(), EditorView.lineWrapping,
    EditorState.allowMultipleSelections.of(true),
    placeholder("从这里开始记录，支持 Markdown…"),
    EditorView.contentAttributes.of({ "aria-label": "笔记正文", spellcheck: "false", tabindex: "0" }),
    EditorView.updateListener.of(publishChanges),
    shortcutsMode.of(shortcutExtensions(bindings)),
    keymap.of(typingKeymap),
    EditorView.domEventHandlers({
      dragover(event) { if (event.dataTransfer?.types.includes("Files")) { event.preventDefault(); return true; } return false; },
      drop(event) { if (event.dataTransfer?.files.length) { event.preventDefault(); return true; } return false; }
    }),
    // Transactions during IME must arrive before the flush barrier. Once frozen,
    // even programmatic commands cannot mutate the acknowledged document.
    EditorState.transactionFilter.of(transaction => transaction.docChanged && transaction.startState.readOnly ? [] : transaction)
  ];
}
const view = new EditorView({
  state: EditorState.create({ doc: "", extensions: extensions(true) }),
  parent: document.querySelector("#editor")!
});

function setReadonly(value: boolean): void {
  view.dispatch({ effects: readOnlyMode.reconfigure(readonlyExtensions(Boolean(value))) });
  updateToolbar();
}
function setSource(value: boolean): void {
  source = Boolean(value);
  view.dispatch({ effects: renderingMode.reconfigure(source ? [] : renderedMarkdown) });
  updateToolbar();
}
function load(value: Load): void {
  if (!Number.isSafeInteger(value?.generation) || value.generation < 0 || typeof value.text !== "string") {
    error("笔记载入参数无效"); return;
  }
  if (stringBytes(value.text) > MAX_DOCUMENT_BYTES) { error("笔记超过 10 MiB，当前内容已保留。"); return; }
  loaded = false;
  unpublished = undefined;
  loadEpoch++;
  generation = value.generation;
  revision = 0;
  pendingFlush = false;
  status.textContent = "";
  status.classList.remove("error", "warning");
  // CM's Text uses LF internally. Only host canonical text owns raw CRLF/mixed EOL.
  view.setState(EditorState.create({ doc: value.text.replace(/\r\n?/g, "\n"), extensions: extensions(Boolean(value.readonly), value.text) }));
  view.scrollDOM.scrollTop = 0;
  loaded = true;
  updateToolbar();
}
async function flush(value: { request: number }): Promise<void> {
  if (!Number.isSafeInteger(value?.request) || value.request < 0) { error("保存请求参数无效"); return; }
  const epoch = loadEpoch;
  const requestGeneration = generation;
  pendingFlush = true;
  updateToolbar();
  // Blurring the focused contenteditable commits a native IME composition.
  // CodeMirror then consumes its pending DOM mutations on the following tasks.
  view.contentDOM.blur();
  const deadline = performance.now() + 2500;
  let stableRevision = revision;
  let stableTicks = 0;
  while (stableTicks < 2) {
    await new Promise(resolve => setTimeout(resolve, 20));
    if (epoch !== loadEpoch) return;
    if (performance.now() > deadline) {
      pendingFlush = false;
      if (unpublished && !view.composing) {
        const rollback = incompleteScalars(view.state.doc);
        if (rollback.length) {
          view.dispatch({ changes: rollback, annotations: Transaction.addToHistory.of(false) });
          const message = "最后一个字符输入未完成，已取消该次字符；请重新输入后再保存。";
          warning(message);
          updateToolbar();
          post({ type: "flushRejected", generation: requestGeneration, request: value.request, message });
          return;
        }
      }
      updateToolbar();
      error("输入法或 Unicode 字符尚未完成，请完成当前输入后重试保存");
      return;
    }
    if (!view.composing && !unpublished && stableRevision === revision) stableTicks++;
    else stableTicks = 0;
    stableRevision = revision;
  }
  setReadonly(true);
  pendingFlush = false;
  updateToolbar();
  post({ type: "flush", generation: requestGeneration, request: value.request, revision, normalizedText: view.state.doc.toString() });
}

toolbar.addEventListener("mousedown", event => {
  // Keep the exact cross-paragraph selection while formatting with the toolbar.
  if ((event.target as Element).closest("button")) event.preventDefault();
});
toolbar.addEventListener("click", event => {
  const button = (event.target as Element).closest<HTMLButtonElement>("button[data-command]");
  if (button && !button.disabled) command(button.dataset.command!);
});
document.addEventListener("keydown", event => {
  // Native divider capture can coexist with WebView keyboard focus. Forward
  // Escape before CodeMirror consumes it, without changing editor semantics.
  if (event.key === "Escape" && !event.isComposing && event.keyCode !== 229 && !view.composing && !event.repeat) {
    post({ type: "cancelResize", generation });
  }
}, { capture: true });
document.addEventListener("keydown", event => {
  // Toolbar focus uses exactly the same configured bindings. Focused content
  // was already handled by the compartment; don't dispatch a second time.
  if (!view.hasFocus) handleShortcut(event, activeShortcutLookup);
});
document.addEventListener("click", event => {
  if ((event.target as Element).closest("a")) event.preventDefault();
});
window.addEventListener("error", event => error(`编辑器错误：${event.message}`));
window.addEventListener("unhandledrejection", () => error("编辑器操作未完成，请保留当前内容并重试"));

const bridge: Bridge = { load, setReadonly, setSource, setShortcuts, command, flush };
if (window.__STUDYAPP_EDITOR_TEST__ === true) {
  const snapshot = () => ({
    generation, revision, text: view.state.doc.toString(), normalizedText: view.state.doc.toString(),
    utf8Bytes: view.state.field(documentBudget).bytes, rawBytes: view.state.field(documentBudget).rawBytes,
    source, readonly: view.state.readOnly, composing: view.composing,
    selection: { anchor: view.state.selection.main.anchor, head: view.state.selection.main.head },
    renderedLines: view.contentDOM.querySelectorAll(".cm-line").length,
    domNodes: view.dom.querySelectorAll("*").length,
    headings: view.contentDOM.querySelectorAll(".md-heading").length,
    highlights: view.contentDOM.querySelectorAll('[class*="tok-"]').length
  });
  bridge.test = async (action: TestAction) => {
    let result: unknown;
    const position = (value: number | undefined) => Math.max(0, Math.min(view.state.doc.length, value ?? 0));
    if (action.type === "snapshot") result = snapshot();
    else if (action.type === "select") {
      view.dispatch({ selection: EditorSelection.range(position(action.anchor), position(action.head ?? action.anchor)), scrollIntoView: true });
      view.focus();
      result = snapshot();
    } else if (action.type === "replace") {
      view.dispatch({
        changes: { from: position(action.from), to: position(action.to ?? action.from), insert: (action.text ?? "").replace(/\r\n?/g, "\n") },
        annotations: Transaction.userEvent.of("input.test")
      });
      result = snapshot();
    } else if (action.type === "command") {
      command(action.name ?? "");
      result = snapshot();
    } else if (action.type === "performance") {
      const edits = Math.max(1, Math.min(1000, action.edits ?? 200));
      const samples: number[] = [];
      view.dispatch({ selection: EditorSelection.cursor(0), scrollIntoView: true });
      view.focus();
      for (let i = 0; i < edits; i++) {
        await new Promise(resolve => requestAnimationFrame(resolve));
        const start = performance.now();
        view.dispatch({ changes: { from: i, insert: "x" }, selection: EditorSelection.cursor(i + 1), annotations: Transaction.userEvent.of("input.type") });
        // Force layout so samples include decoration/DOM update cost, not only state changes.
        view.contentDOM.getBoundingClientRect();
        samples.push(performance.now() - start);
      }
      samples.sort((a, b) => a - b);
      result = { edits, medianMs: samples[Math.floor(samples.length / 2)], p95Ms: samples[Math.ceil(samples.length * .95) - 1],
        renderedLines: view.contentDOM.querySelectorAll(".cm-line").length, domNodes: view.dom.querySelectorAll("*").length };
    } else throw new Error(`Unknown test action: ${action.type}`);
    if (action.request !== undefined) post({ type: "test", generation, request: action.request, result });
    return result;
  };
}
window.studyappEditor = bridge;
updateToolbar();
post({ type: "ready" });
