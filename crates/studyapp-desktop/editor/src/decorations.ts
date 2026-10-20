import { Range, Transaction } from "@codemirror/state";
import { syntaxTree } from "@codemirror/language";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate, WidgetType } from "@codemirror/view";

class BulletWidget extends WidgetType {
  eq() { return true; }
  toDOM() {
    const span = document.createElement("span");
    span.className = "md-bullet";
    span.textContent = "•";
    span.setAttribute("aria-hidden", "true");
    return span;
  }
}

class TaskWidget extends WidgetType {
  constructor(readonly from: number, readonly checked: boolean, readonly disabled: boolean) { super(); }
  eq(other: TaskWidget) { return this.from === other.from && this.checked === other.checked && this.disabled === other.disabled; }
  toDOM() {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "md-task";
    button.setAttribute("role", "checkbox");
    button.setAttribute("aria-checked", String(this.checked));
    button.setAttribute("aria-label", "切换任务完成状态");
    button.dataset.taskFrom = String(this.from);
    button.disabled = this.disabled;
    button.textContent = this.checked ? "✓" : "";
    button.addEventListener("keydown", event => {
      if (event.key === "Enter" || event.key === " ") {
        // A widget button lives inside the editor's event tree. Consume these
        // before CodeMirror interprets Enter/Space as text at its old cursor.
        event.preventDefault();
        event.stopPropagation();
        button.click();
      }
    });
    return button;
  }
  ignoreEvent() { return false; }
}

export const renderedMarkdown = ViewPlugin.fromClass(class {
  decorations: DecorationSet;
  tree;
  constructor(view: EditorView) {
    this.tree = syntaxTree(view.state);
    this.decorations = this.build(view);
  }
  update(update: ViewUpdate) {
    const tree = syntaxTree(update.state);
    if (update.docChanged || update.selectionSet || update.viewportChanged || tree !== this.tree ||
        update.startState.readOnly !== update.state.readOnly) {
      this.tree = tree;
      this.decorations = this.build(update.view);
    }
  }
  build(view: EditorView): DecorationSet {
    const ranges: Range<Decoration>[] = [];
    const lines = new Set<string>();
    const seen = new Set<string>();
    const { state } = view;
    const active = (from: number, to: number) => state.selection.ranges.some(range => range.from <= to && range.to >= from);
    const mark = (from: number, to: number, className: string, attributes?: Record<string, string>) => {
      if (to > from) ranges.push(Decoration.mark({ class: className, attributes }).range(from, to));
    };
    const hide = (from: number, to: number) => {
      // ViewPlugin replacements may never remove line breaks. Markdown link
      // titles can span lines, so retain each newline and hide only line text.
      for (let position = from; position < to;) {
        const current = state.doc.lineAt(position);
        const end = Math.min(to, current.to);
        if (end > position) ranges.push(Decoration.replace({}).range(position, end));
        position = current.to + 1;
      }
    };
    const line = (position: number, className: string) => {
      const from = state.doc.lineAt(position).from;
      const key = `${from}:${className}`;
      if (!lines.has(key)) {
        lines.add(key);
        ranges.push(Decoration.line({ class: className }).range(from));
      }
    };
    for (const visible of view.visibleRanges) {
      this.tree.iterate({
        from: visible.from, to: visible.to,
        enter: cursor => {
          const { name, from, to } = cursor;
          const key = `${name}:${from}:${to}`;
          if (seen.has(key)) return;
          seen.add(key);
          const node = cursor.node;
          const parent = node.parent;
          if (/^(ATXHeading|SetextHeading)[1-6]$/.test(name)) {
            line(from, `md-heading md-heading-${name.at(-1)}`);
          } else if (name === "StrongEmphasis") {
            mark(from, to, "md-strong");
          } else if (name === "Emphasis") {
            mark(from, to, "md-emphasis");
          } else if (name === "Strikethrough") {
            mark(from, to, "md-strike");
          } else if (name === "InlineCode") {
            mark(from, to, "md-inline-code");
          } else if (name === "Blockquote") {
            const first = state.doc.lineAt(Math.max(from, visible.from)).number;
            const last = state.doc.lineAt(Math.min(to, visible.to)).number;
            for (let i = first; i <= last; i++) line(state.doc.line(i).from, "md-quote");
          } else if (name === "FencedCode" || name === "CodeBlock") {
            const first = state.doc.lineAt(Math.max(from, visible.from)).number;
            const last = state.doc.lineAt(Math.min(to, visible.to)).number;
            for (let i = first; i <= last; i++) line(state.doc.line(i).from, "md-code-line");
          } else if (name === "HeaderMark" && parent && !active(parent.from, parent.to)) {
            const end = state.doc.lineAt(to).to;
            hide(from, Math.min(end, to + (state.sliceDoc(to, to + 1) === " " ? 1 : 0)));
          } else if (["EmphasisMark", "StrikethroughMark", "CodeMark"].includes(name) && parent && !active(parent.from, parent.to)) {
            hide(from, to);
            if (parent.name === "FencedCode") line(from, "md-code-fence");
          } else if (name === "CodeInfo" && parent && !active(parent.from, parent.to)) {
            mark(from, to, "md-code-language");
          } else if (name === "ListMark") {
            line(from, "md-list-line");
            const current = state.doc.lineAt(from);
            if (!active(current.from, current.to) && /^[-+*]$/.test(state.sliceDoc(from, to))) {
              const task = parent?.getChild("Task");
              if (task) hide(from, Math.min(to + 1, current.to));
              else ranges.push(Decoration.replace({ widget: new BulletWidget() }).range(from, to));
            } else mark(from, to, "md-list-marker");
          } else if (name === "QuoteMark") {
            const current = state.doc.lineAt(from);
            if (!active(current.from, current.to)) hide(from, Math.min(to + 1, current.to));
          } else if (name === "TaskMarker") {
            const current = state.doc.lineAt(from);
            if (!active(current.from, current.to)) {
              ranges.push(Decoration.replace({ widget: new TaskWidget(from, /\[[xX]\]/.test(state.sliceDoc(from, to)), state.readOnly) }).range(from, to));
            }
          } else if (name === "Link" || name === "Autolink") {
            const urlNode = node.getChild("URL");
            const url = urlNode ? state.sliceDoc(urlNode.from, urlNode.to) : "";
            const safe = /^(https?:\/\/|mailto:)/i.test(url);
            mark(from, to, "md-link", { title: safe ? `${url}（选中以编辑链接）` : "选中以编辑链接" });
            if (!active(from, to) && name === "Link") {
              for (let child = node.firstChild; child; child = child.nextSibling) {
                if (["LinkMark", "URL", "LinkTitle"].includes(child.name)) hide(child.from, child.to);
              }
            }
          } else if (name === "TableHeader" || name === "TableRow") {
            line(from, `md-table-row${name === "TableHeader" ? " md-table-header" : ""}`);
            const cells = node.getChildren("TableCell");
            const columns = parent?.getChild("TableHeader")?.getChildren("TableCell").length || cells.length;
            if (!active(from, to)) {
              for (const cell of cells) mark(cell.from, cell.to, "md-table-cell", { style: `width:calc((100% - ${columns * 26}px) / ${columns})` });
            }
          } else if (name === "TableDelimiter") {
            const current = state.doc.lineAt(from);
            if (!active(current.from, current.to)) {
              hide(from, to);
              if (parent?.name === "Table") line(from, "md-table-separator");
            }
          } else if (name === "HorizontalRule") {
            line(from, "md-rule");
            if (!active(from, to)) hide(from, to);
          }
        }
      });
    }
    return Decoration.set(ranges, true);
  }
}, {
  decorations: plugin => plugin.decorations,
  eventHandlers: {
    mousedown(event) {
      if ((event.target as Element).closest(".md-task")) { event.preventDefault(); return true; }
      return false;
    },
    click(event, view) {
      const target = (event.target as Element).closest<HTMLButtonElement>("[data-task-from]");
      if (!target || view.state.readOnly) return false;
      const from = Number(target.dataset.taskFrom);
      const original = view.state.sliceDoc(from, from + 3);
      if (!/^\[[ xX]\]$/.test(original)) return false;
      const hadFocus = document.activeElement === target;
      view.dispatch({
        changes: { from: from + 1, to: from + 2, insert: original[1] === " " ? "x" : " " },
        annotations: Transaction.userEvent.of("input.task")
      });
      if (hadFocus) view.dom.querySelector<HTMLButtonElement>(`[data-task-from="${from}"]`)?.focus({ preventScroll: true });
      event.preventDefault();
      return true;
    }
  }
});
