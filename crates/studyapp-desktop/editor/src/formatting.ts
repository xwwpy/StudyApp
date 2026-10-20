import { EditorSelection, Transaction, ChangeSpec } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { undo, redo, isolateHistory } from "@codemirror/commands";
import { syntaxTree } from "@codemirror/language";

function surround(view: EditorView, delimiter: string): boolean {
  const { state } = view;
  const range = state.selection.main;
  const width = delimiter.length;
  const hasBefore = (position: number) => position >= width && state.sliceDoc(position - width, position) === delimiter;
  const hasAfter = (position: number) => state.sliceDoc(position, position + width) === delimiter;
  if (range.empty) {
    const remove = hasBefore(range.from) && hasAfter(range.to);
    view.dispatch({
      changes: remove ? { from: range.from - width, to: range.to + width } : { from: range.from, insert: delimiter + delimiter },
      selection: EditorSelection.cursor(range.from + (remove ? -width : width)),
      annotations: [Transaction.userEvent.of("input.format"), isolateHistory.of("full")],
      scrollIntoView: true
    });
    return true;
  }
  const first = state.doc.lineAt(range.from);
  const last = state.doc.lineAt(range.to - 1);
  const segments: { from: number; to: number; contentFrom: number; contentTo: number; active: boolean; marker: string; padding: string }[] = [];
  const type = delimiter === "**" ? "StrongEmphasis" : delimiter === "*" ? "Emphasis" : delimiter === "~~" ? "Strikethrough" : "InlineCode";
  const markerType = delimiter === "`" ? "CodeMark" : delimiter === "~~" ? "StrikethroughMark" : "EmphasisMark";
  const onlyMarkersBetween = (a: number, b: number) => /^[*_~`]*$/.test(state.sliceDoc(Math.min(a, b), Math.max(a, b)));
  for (let number = first.number; number <= last.number; number++) {
    const line = state.doc.line(number);
    // Keep heading/list/quote/task structure outside inline formatting. Each
    // selected nonempty line gets its own markers, so blank paragraphs remain
    // valid Markdown and one toolbar action is still one undo transaction.
    const prefix = line.text.match(/^[ \t]*(?:(?:#{1,6}|>|[-+*]|\d+[.)])[ \t]+(?:\[[ xX]\][ \t]+)?)*/)?.[0] ?? "";
    let from = Math.max(range.from, line.from + prefix.length);
    let to = Math.min(range.to, line.to);
    while (from < to && /[ \t]/.test(state.sliceDoc(from, from + 1))) from++;
    while (to > from && /[ \t]/.test(state.sliceDoc(to - 1, to))) to--;
    if (from >= to) continue;
    const selected = state.sliceDoc(from, to);
    const segment = { from, to, contentFrom: from, contentTo: to, active: false, marker: delimiter, padding: "" };
    // Consult the Markdown parse tree: a '*' that belongs to '**' is not an
    // existing italic marker. Nested emphasis must retain its other wrapper.
    syntaxTree(state).iterate({
      from, to,
      enter(node) {
        if (node.name !== type || segment.active) return;
        const marks = node.node.getChildren(markerType);
        const opening = marks[0], closing = marks.at(-1);
        if (!opening || !closing || opening === closing) return;
        let contentFrom = opening.to, contentTo = closing.from;
        let active: boolean;
        if (delimiter === "`") {
          const content = state.sliceDoc(contentFrom, contentTo);
          // CommonMark removes one leading/trailing space from code spans
          // when both are present and the content is not entirely spaces.
          // These are also the protective spaces inserted around backticks.
          if (content.startsWith(" ") && content.endsWith(" ") && /[^ ]/.test(content)) {
            contentFrom++;
            contentTo--;
          }
          active = [opening.from, opening.to, contentFrom].includes(from) &&
            [closing.to, closing.from, contentTo].includes(to);
        } else {
          active = from <= closing.from && to >= opening.to &&
            onlyMarkersBetween(opening.to, from) && onlyMarkersBetween(to, closing.from);
        }
        if (active) {
          segment.from = opening.from; segment.to = closing.to;
          segment.contentFrom = contentFrom; segment.contentTo = contentTo;
          segment.active = true;
        }
      }
    });
    if (delimiter === "`" && !segment.active) {
      let longestRun = 0;
      for (const match of selected.matchAll(/`+/g)) longestRun = Math.max(longestRun, match[0].length);
      segment.marker = "`".repeat(longestRun + 1);
      segment.padding = selected.startsWith("`") || selected.endsWith("`") ? " " : "";
    }
    segments.push(segment);
  }
  if (!segments.length) return false;
  const remove = segments.every(segment => segment.active);
  const edits: ChangeSpec[] = [];
  for (const segment of segments) {
    if (remove) {
      edits.push({ from: segment.from, to: segment.contentFrom }, { from: segment.contentTo, to: segment.to });
    } else if (!segment.active) {
      edits.push({ from: segment.from, insert: segment.marker + segment.padding }, { from: segment.to, insert: segment.padding + segment.marker });
    }
  }
  const changes = state.changes(edits);
  const singleLine = first.number === last.number;
  const mappedFrom = changes.mapPos(singleLine ? segments[0].contentFrom : range.from, singleLine ? 1 : -1);
  const mappedTo = changes.mapPos(singleLine ? segments[0].contentTo : range.to, singleLine ? -1 : 1);
  view.dispatch({
    changes,
    selection: EditorSelection.range(range.anchor <= range.head ? mappedFrom : mappedTo, range.anchor <= range.head ? mappedTo : mappedFrom),
    annotations: [Transaction.userEvent.of("input.format"), isolateHistory.of("full")],
    scrollIntoView: true
  });
  return true;
}

function prefixLines(view: EditorView, prefix: string): boolean {
  const { state } = view;
  const selection = state.selection.main;
  const first = state.doc.lineAt(selection.from);
  const last = state.doc.lineAt(selection.to > selection.from ? selection.to - 1 : selection.to);
  const changes: ChangeSpec[] = [];
  let remove = true;
  for (let number = first.number; number <= last.number; number++) {
    if (!state.doc.line(number).text.startsWith(prefix)) remove = false;
  }
  for (let number = first.number; number <= last.number; number++) {
    const line = state.doc.line(number);
    changes.push(remove ? { from: line.from, to: line.from + prefix.length } : { from: line.from, insert: prefix });
  }
  view.dispatch({ changes, annotations: [Transaction.userEvent.of("input.format"), isolateHistory.of("full")], scrollIntoView: true });
  return true;
}

export function formatCommand(view: EditorView, name: string): boolean {
  if (view.state.readOnly) return false;
  if (name === "undo") return undo(view);
  if (name === "redo") return redo(view);
  if (name === "bold") return surround(view, "**");
  if (name === "italic") return surround(view, "*");
  if (name === "code") return surround(view, "`");
  if (name === "strike") return surround(view, "~~");
  if (name === "heading") return prefixLines(view, "## ");
  if (name === "bullet") return prefixLines(view, "- ");
  if (name === "task") return prefixLines(view, "- [ ] ");
  if (name === "quote") return prefixLines(view, "> ");
  return false;
}
