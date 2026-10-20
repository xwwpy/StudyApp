import { Compartment, EditorSelection, EditorState, Extension, Transaction } from "@codemirror/state";
import { drawSelection, EditorView, keymap, placeholder, ViewUpdate } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { syntaxHighlighting } from "@codemirror/language";
import { codeHighlighting, fencedLanguage } from "./languages";
import { renderedMarkdown } from "./decorations";
import { formatCommand } from "./formatting";
import { documentBudget, documentLimits, MAX_DOCUMENT_BYTES, stringBytes } from "./limits";

interface Load { generation: number; text: string; readonly: boolean; }
interface TestAction { type: string; request?: string | number; from?: number; to?: number; text?: string; anchor?: number; head?: number; name?: string; edits?: number; }
interface Bridge {
  load(value: Load): void;
  setReadonly(value: boolean): void;
  setSource(value: boolean): void;
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
let generation = 0;
let revision = 0;
let source = false;
let loaded = false;
let loadEpoch = 0;
let pendingFlush = false;
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
  }
  sourceButton.setAttribute("aria-pressed", String(source));
  if (!status.classList.contains("error") && !status.classList.contains("warning")) status.textContent = pendingFlush ? "正在确认输入…" : view.state.readOnly ? "只读" : "";
}
function readonlyExtensions(value: boolean): Extension {
  return [EditorState.readOnly.of(value), EditorView.editable.of(!value)];
}
function publishChanges(update: ViewUpdate): void {
  if (!loaded) return;
  for (const transaction of update.transactions) {
    if (!transaction.docChanged) continue;
    const changes: { fromLine: number; fromColumn: number; toLine: number; toColumn: number; text: string }[] = [];
    transaction.changes.iterChanges((from, to, _newFrom, _newTo, inserted) => {
      const first = transaction.startState.doc.lineAt(from);
      const last = transaction.startState.doc.lineAt(to);
      changes.push({
        fromLine: first.number, fromColumn: from - first.from + 1,
        toLine: last.number, toColumn: to - last.from + 1, text: inserted.toString()
      });
    });
    const baseRevision = revision++;
    post({ type: "change", generation, baseRevision, revision, changes });
  }
}
function command(name: string): boolean {
  if (name === "source") { setSource(!source); return true; }
  if (name === "save") { post({ type: "save", generation, revision }); return true; }
  if (pendingFlush) return false;
  const result = formatCommand(view, name);
  if (result) view.focus();
  return result;
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
    keymap.of([
      { key: "Mod-s", run: () => command("save"), preventDefault: true },
      { key: "Mod-b", run: () => command("bold") },
      { key: "Mod-i", run: () => command("italic") },
      ...historyKeymap, ...defaultKeymap, indentWithTab
    ]),
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
      updateToolbar();
      error("输入法尚未结束，请完成当前输入后重试保存");
      return;
    }
    if (!view.composing && stableRevision === revision) stableTicks++;
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
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    // Editor keymap consumes this first when focused; defaultPrevented does not
    // indicate consumption here because it was just set by this listener.
    if (!view.hasFocus) command("save");
  }
});
document.addEventListener("click", event => {
  if ((event.target as Element).closest("a")) event.preventDefault();
});
window.addEventListener("error", event => error(`编辑器错误：${event.message}`));
window.addEventListener("unhandledrejection", () => error("编辑器操作未完成，请保留当前内容并重试"));

const bridge: Bridge = { load, setReadonly, setSource, command, flush };
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
