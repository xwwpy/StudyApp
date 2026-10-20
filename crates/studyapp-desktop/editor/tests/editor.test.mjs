import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const root = fileURLToPath(new URL("../dist/", import.meta.url));
let server, browser, base;
before(async () => {
  const assets = new Map();
  for (const name of ["index.html", "editor.js", "editor.css"]) {
    try { assets.set(name, await readFile(root + name)); }
    catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  server = createServer(async (req, res) => {
    const name = new URL(req.url, "http://localhost").pathname.slice(1) || "index.html";
    if (!["index.html", "editor.js", "editor.css"].includes(name)) {
      res.writeHead(404).end(); return;
    }
    if (assets.has(name)) {
      res.setHeader("Content-Type", name.endsWith(".js") ? "text/javascript" : name.endsWith(".css") ? "text/css" : "text/html");
      res.end(assets.get(name));
    } else {
      res.setHeader("Content-Type", "text/html");
      res.end("<!doctype html><title>Frontend is not implemented</title>");
    }
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  base = `http://127.0.0.1:${server.address().port}/`;
  const edge = "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
  browser = await chromium.launch({ headless: true, ...(existsSync(edge) ? { executablePath: edge } : {}) });
});
after(async () => { await browser?.close(); await new Promise(resolve => server?.close(resolve)); });

async function setup(text = "", testEnabled = true) {
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 } });
  const errors = [];
  const externalRequests = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => { if (!request.url().startsWith(base)) externalRequests.push(request.url()); });
  await page.addInitScript(({ testEnabled }) => {
    window.__STUDYAPP_EDITOR_TEST__ = testEnabled;
    window.messages = [];
    window.ipc = { postMessage: message => window.messages.push(JSON.parse(message)) };
  }, { testEnabled });
  await page.goto(base);
  const api = await page.evaluate(() => typeof window.studyappEditor);
  assert.equal(api, "object", `offline editor bridge must exist; page errors: ${JSON.stringify(errors)}; html: ${api !== "object" ? (await page.content()).slice(0, 600) : ""}`);
  await page.evaluate(text => window.studyappEditor.load({ generation: 41, text, readonly: false }), text);
  await page.waitForTimeout(80);
  return { page, errors, externalRequests };
}
async function action(page, value) { return page.evaluate(value => window.studyappEditor.test(value), value); }
async function state(page) { return action(page, { type: "snapshot" }); }
async function select(page, anchor, head = anchor) { return action(page, { type: "select", anchor, head }); }
function replayRawChanges(raw, messages, preferredEol) {
  for (const [index, message] of messages.entries()) {
    assert.ok(message.changes.length <= 4096, `emitted ${message.changes.length} patches`);
    assert.equal(message.baseRevision, index);
    assert.equal(message.revision, index + 1);
    const starts = [0];
    for (const match of raw.matchAll(/\r\n|\r|\n/g)) starts.push(match.index + match[0].length);
    const edits = message.changes.map(change => ({
      from: starts[change.fromLine - 1] + change.fromColumn - 1,
      to: starts[change.toLine - 1] + change.toColumn - 1,
      text: change.text.replace(/\n/g, preferredEol)
    }));
    for (const edit of edits.reverse()) raw = raw.slice(0, edit.from) + edit.text + raw.slice(edit.to);
    assert.ok(raw.isWellFormed(), "native revision must remain scalar-valid");
    assert.ok(Buffer.byteLength(raw) <= 10 * 1024 * 1024, "native revision exceeds the byte budget");
  }
  return raw;
}

test("bundled editor starts with one continuous editable Markdown surface", async () => {
  const { page, errors } = await setup("# 连续编辑\n\n正文包含 **粗体** 和 *强调*。\n\n- 第一项\n- 第二项\n\n结尾");
  assert.equal(await page.locator('[contenteditable="true"]').count(), 1);
  assert.equal(await page.locator("textarea").count(), 0);
  await select(page, 52);
  assert.ok(await page.locator(".md-heading").count());
  assert.ok(await page.locator(".md-strong").count());
  assert.ok(await page.locator(".md-emphasis").count());
  assert.ok(await page.locator(".md-bullet").count());
  const visible = await page.locator(".cm-content").innerText();
  assert.ok(!visible.includes("**粗体**"));
  assert.ok(!visible.includes("# 连续编辑"));
  await select(page, "# 连续编辑\n\n正文包含 **粗".length);
  assert.ok((await page.locator(".cm-content").innerText()).includes("**粗体**"));
  assert.deepEqual(errors, []);
  await page.close();
});

test("Chinese prose uses spacious body typography inside the single scroll viewport", async () => {
  const text = "# 连续记录，更专注\n\n直接编辑这份笔记，**重点内容** 与 *强调* 在原位置呈现。\n\n## 一个清晰的工作区\n\n- 阅读与书写在同一处\n- [x] 支持任务、列表和跨段选择\n- [ ] 支持中文输入法\n\n> 保留原始 Markdown，编辑体验更自然。\n\n| 语言 | 状态 |\n| :--- | :--- |\n| Rust / Python | 离线高亮 |\n| JavaScript / C++ | 离线高亮 |\n\n```rust\nfn main() {\n    let progress = 42;\n    println!(\"Keep learning: {progress}\");\n}\n```\n\n";
  const { page } = await setup(text);
  await select(page, text.length);
  const appearance = await page.evaluate(() => ({
    font: getComputedStyle(document.querySelector(".cm-scroller")).fontFamily,
    padding: parseFloat(getComputedStyle(document.querySelector(".cm-content")).paddingLeft),
    scrollHeight: document.querySelector(".cm-scroller").clientHeight,
    documentOverflow: document.documentElement.scrollHeight > window.innerHeight
  }));
  assert.ok(appearance.font.includes("Segoe UI"), JSON.stringify(appearance));
  assert.ok(appearance.padding >= 26, JSON.stringify(appearance));
  assert.ok(appearance.scrollHeight < 800 && appearance.scrollHeight > 700);
  assert.equal(appearance.documentOverflow, false);
  await mkdir(new URL("../test-results/", import.meta.url), { recursive: true });
  await page.screenshot({ path: fileURLToPath(new URL("../test-results/sample-editor.png", import.meta.url)) });
  await page.close();
});

test("cross-paragraph input, Unicode UTF-16 patches, undo and source preserve history", async () => {
  const { page } = await setup("# 标题\r\n\r\n甲😀乙\r\n第二段\r末段");
  const initial = "# 标题\n\n甲😀乙\n第二段\n末段";
  assert.equal((await state(page)).text, initial);
  await select(page, initial.indexOf("😀"), initial.indexOf("末段"));
  await page.keyboard.insertText("新的😀\n跨段");
  const edited = (await state(page)).text;
  const editedSelection = (await state(page)).selection;
  assert.equal(edited, "# 标题\n\n甲新的😀\n跨段末段");
  const changes = await page.evaluate(() => window.messages.filter(m => m.type === "change"));
  assert.equal(changes.length, 1);
  assert.deepEqual(changes[0], {
    type: "change", generation: 41, baseRevision: 0, revision: 1,
    changes: [{ fromLine: 3, fromColumn: 2, toLine: 5, toColumn: 1, text: "新的😀\n跨段" }]
  });
  await page.evaluate(() => window.studyappEditor.setSource(true));
  assert.equal(await page.locator(".md-heading").count(), 0);
  assert.equal((await state(page)).text, edited);
  assert.deepEqual((await state(page)).selection, editedSelection);
  await page.evaluate(() => window.studyappEditor.setSource(false));
  await page.evaluate(() => window.studyappEditor.command("undo"));
  assert.equal((await state(page)).text, initial);
  await page.evaluate(() => window.studyappEditor.command("redo"));
  assert.equal((await state(page)).text, edited);
  await page.close();
});

test("toolbar formats selections spanning paragraphs and supports task interaction", async () => {
  const { page } = await setup("第一段\n\n第二段\n\n- [ ] 待办\n\n结尾");
  await select(page, 0, 8);
  await page.getByRole("button", { name: "粗体" }).click();
  assert.equal((await state(page)).text.slice(0, 16), "**第一段**\n\n**第二段**");
  assert.equal(await page.locator(".md-strong").count(), 2);
  await page.getByRole("button", { name: "粗体" }).click();
  assert.equal((await state(page)).text, "第一段\n\n第二段\n\n- [ ] 待办\n\n结尾");
  await page.evaluate(() => window.studyappEditor.command("undo"));
  await page.evaluate(() => window.studyappEditor.command("undo"));
  await select(page, (await state(page)).text.length);
  await page.getByRole("checkbox", { name: "切换任务完成状态" }).click();
  assert.ok((await state(page)).text.includes("- [x] 待办"));
  await page.evaluate(() => window.studyappEditor.command("undo"));
  assert.ok((await state(page)).text.includes("- [ ] 待办"));
  await page.close();
});

test("split native UTF-16 input publishes one scalar-safe patch and exact byte budget", async () => {
  const { page } = await setup("甲\r\n乙");
  await action(page, { type: "replace", from: 1, text: "\uD83E" });
  assert.equal(await page.evaluate(() => window.messages.filter(m => m.type === "change").length), 0,
    "a lone high surrogate must never be sent to Rust JSON");
  await action(page, { type: "replace", from: 2, text: "\uDDE0" });
  assert.equal((await state(page)).text, "甲🧠\n乙");
  const messages = await page.evaluate(() => window.messages.filter(m => m.type === "change"));
  assert.deepEqual(messages, [{
    type: "change", generation: 41, baseRevision: 0, revision: 1,
    changes: [{ fromLine: 1, fromColumn: 2, toLine: 1, toColumn: 2, text: "🧠" }]
  }]);
  assert.equal((await state(page)).utf8Bytes, Buffer.byteLength("甲🧠\n乙"));
  assert.equal((await state(page)).rawBytes, Buffer.byteLength("甲🧠\r\n乙"));
  await page.evaluate(() => window.studyappEditor.flush({ request: 91 }));
  const flush = await page.evaluate(() => window.messages.find(m => m.type === "flush"));
  assert.equal(flush.revision, 1);
  assert.equal(flush.normalizedText, "甲🧠\n乙");
  await page.close();
});

test("flush waits for split native input and a new document drops pending old-generation text", async () => {
  const { page } = await setup("");
  await action(page, { type: "replace", from: 0, text: "\uD83E" });
  await page.evaluate(() => {
    window.flushPending = window.studyappEditor.flush({ request: 92 });
    setTimeout(() => window.studyappEditor.test({ type: "replace", from: 1, text: "\uDDE0" }), 80);
  });
  await page.evaluate(() => window.flushPending);
  assert.equal((await state(page)).text, "🧠");
  assert.equal(await page.evaluate(() => window.messages.find(m => m.type === "flush").normalizedText), "🧠");
  await page.evaluate(() => window.studyappEditor.load({ generation: 42, text: "", readonly: false }));
  await action(page, { type: "replace", from: 0, text: "\uD83E" });
  await page.evaluate(() => window.studyappEditor.load({ generation: 43, text: "新文档", readonly: false }));
  await action(page, { type: "replace", from: 3, text: "。" });
  const last = await page.evaluate(() => window.messages.filter(m => m.type === "change").at(-1));
  assert.equal(last.generation, 43);
  assert.equal(last.baseRevision, 0);
  assert.deepEqual(last.changes, [{ fromLine: 1, fromColumn: 4, toLine: 1, toColumn: 4, text: "。" }]);
  await page.close();
});

test("composed native Unicode edits stay within the bridge patch cap and preserve mixed EOLs", async () => {
  const lines = Array.from({ length: 2048 }, (_, index) => `line ${index}`);
  const separators = lines.map((_, index) => index % 5 === 0 ? "\n" : index % 7 === 0 ? "\r" : "\r\n");
  const original = "head\r\n" + lines.map((line, index) => line + separators[index]).join("") + "tail";
  const { page } = await setup(original);
  await action(page, { type: "replace", from: 1, text: "\uD83E" });
  const pending = (await state(page)).text;
  await select(page, pending.indexOf("line 0"), pending.lastIndexOf("\ntail"));
  await page.evaluate(() => window.studyappEditor.command("bold"));
  assert.equal(await page.evaluate(() => window.messages.filter(m => m.type === "change").length), 0);
  await action(page, { type: "replace", from: 2, text: "\uDDE0" });
  const messages = await page.evaluate(() => window.messages.filter(m => m.type === "change"));
  assert.ok(messages.length > 0);
  const raw = replayRawChanges(original, messages, "\r\n");
  const expected = "h🧠ead\r\n" + lines.map((line, index) => `**${line}**` + separators[index]).join("") + "tail";
  assert.equal(raw, expected);
  assert.equal((await state(page)).rawBytes, Buffer.byteLength(expected));
  await page.evaluate(() => window.studyappEditor.flush({ request: 96 }));
  const flush = await page.evaluate(() => window.messages.find(m => m.type === "flush"));
  assert.equal(flush.revision, messages.length);
  assert.equal(flush.normalizedText, expected.replace(/\r\n?|\n/g, "\n"));
  await page.close();
});

test("composed native patch batches respect the raw byte limit at every intermediate revision", async () => {
  const limit = 10 * 1024 * 1024;
  const lines = Array.from({ length: 2048 }, (_, index) => `line ${index}`);
  const separators = lines.map((_, index) => index % 5 === 0 ? "\n" : index % 7 === 0 ? "\r" : "\r\n");
  const suffix = lines.map((line, index) => line + separators[index]).join("") + "tail";
  const prefix = "x".repeat(limit - 3 - Buffer.byteLength(suffix) - 2);
  const original = prefix + "\r\n" + suffix;
  const { page } = await setup(original);
  await action(page, { type: "replace", from: (await state(page)).text.length, text: "\uD83E" });
  await action(page, { type: "replace", from: 0, to: 8193, text: "" });
  const pending = (await state(page)).text;
  await select(page, pending.indexOf("line 0"), pending.lastIndexOf("\ntail"));
  await page.evaluate(() => window.studyappEditor.command("bold"));
  await action(page, { type: "replace", from: (await state(page)).text.length, text: "\uDDE0" });
  const messages = await page.evaluate(() => window.messages.filter(m => m.type === "change"));
  const raw = replayRawChanges(original, messages, "\r\n");
  const expected = prefix.slice(8193) + "\r\n" + lines.map((line, index) => `**${line}**` + separators[index]).join("") + "tail🧠";
  assert.ok(raw === expected, "batched revisions must preserve the complete mixed-EOL document");
  assert.equal(Buffer.byteLength(raw), limit);
  assert.equal((await state(page)).rawBytes, limit);
  assert.equal((await state(page)).readonly, false);
  await page.evaluate(() => window.studyappEditor.flush({ request: 97 }));
  const flush = await page.evaluate(() => window.messages.find(m => m.type === "flush"));
  assert.equal(flush.revision, messages.length);
  assert.ok(flush.normalizedText === expected.replace(/\r\n?|\n/g, "\n"));
  await page.close();
});

test("undo and redo across separate surrogate history groups publish only complete characters", async () => {
  const { page } = await setup("甲乙");
  await action(page, { type: "replace", from: 1, text: "\uD83E" });
  await page.waitForTimeout(600);
  await action(page, { type: "replace", from: 2, text: "\uDDE0" });
  await page.evaluate(() => window.studyappEditor.command("undo"));
  assert.equal((await state(page)).text, "甲\uD83E乙");
  assert.equal(await page.evaluate(() => window.messages.filter(m => m.type === "change").length), 1);
  await page.evaluate(() => window.studyappEditor.command("undo"));
  assert.equal((await state(page)).text, "甲乙");
  await page.evaluate(() => window.studyappEditor.command("redo"));
  assert.equal((await state(page)).text, "甲\uD83E乙");
  await page.evaluate(() => window.studyappEditor.command("redo"));
  assert.equal((await state(page)).text, "甲🧠乙");
  const messages = await page.evaluate(() => window.messages.filter(m => m.type === "change"));
  assert.equal(messages.length, 3);
  assert.deepEqual(messages.map(m => m.revision), [1, 2, 3]);
  assert.deepEqual(messages[1].changes, [{ fromLine: 1, fromColumn: 2, toLine: 1, toColumn: 4, text: "" }]);
  assert.equal((await state(page)).rawBytes, Buffer.byteLength("甲🧠乙"));
  await page.close();
});

test("incomplete native Unicode input never receives a save acknowledgement", async () => {
  const { page } = await setup("保留");
  await page.evaluate(() => {
    const original = window.ipc.postMessage;
    window.ipc.postMessage = json => {
      original(json);
      if (JSON.parse(json).type === "error") setTimeout(() => window.studyappEditor.setReadonly(true), 0);
    };
  });
  await action(page, { type: "replace", from: 2, text: "\uD83E" });
  await action(page, { type: "replace", from: 3, text: "后续" });
  await page.evaluate(() => window.studyappEditor.flush({ request: 93 }));
  assert.equal((await state(page)).text, "保留后续");
  assert.equal(await page.evaluate(() => window.messages.some(m => m.type === "flush")), false);
  assert.deepEqual(await page.evaluate(() => window.messages.filter(m => m.type === "change").flatMap(m => m.changes)),
    [{ fromLine: 1, fromColumn: 3, toLine: 1, toColumn: 3, text: "后续" }]);
  assert.equal(await page.evaluate(() => window.messages.some(m => m.type === "flushRejected" && m.generation === 41 && m.request === 93)), true);
  assert.equal((await state(page)).readonly, false);
  await action(page, { type: "replace", from: 4, text: "🧠" });
  await page.evaluate(() => window.studyappEditor.flush({ request: 94 }));
  assert.equal(await page.evaluate(() => window.messages.find(m => m.type === "flush").normalizedText), "保留后续🧠");
  await page.close();
});

test("over-limit native emoji rolls back only its incomplete units and leaves editing usable", async () => {
  const original = "x".repeat(10 * 1024 * 1024 - 3);
  const { page } = await setup(original);
  await action(page, { type: "replace", from: original.length, text: "\uD83E" });
  await action(page, { type: "replace", from: original.length + 1, text: "\uDDE0" });
  assert.ok((await state(page)).text === original, "over-limit pair must restore the valid document");
  assert.equal((await state(page)).rawBytes, original.length);
  assert.equal((await state(page)).readonly, false);
  await action(page, { type: "replace", from: 0, to: 1, text: "" });
  await action(page, { type: "replace", from: original.length - 1, text: "🧠" });
  await page.evaluate(() => window.studyappEditor.flush({ request: 95 }));
  assert.equal(await page.evaluate(() => window.messages.find(m => m.type === "flush").normalizedText.endsWith("🧠")), true);
  await page.close();
});

test("inline formatting toggles and preserves heading/list prefixes across selected lines", async () => {
  const { page } = await setup("# heading\n\n- item\n\nword");
  await select(page, 0, 17);
  await page.evaluate(() => window.studyappEditor.command("bold"));
  assert.equal((await state(page)).text, "# **heading**\n\n- **item**\n\nword");
  await page.evaluate(() => window.studyappEditor.command("undo"));
  await select(page, 19, 23);
  await page.evaluate(() => window.studyappEditor.command("italic"));
  assert.equal((await state(page)).text, "# heading\n\n- item\n\n*word*");
  await page.evaluate(() => window.studyappEditor.command("italic"));
  assert.equal((await state(page)).text, "# heading\n\n- item\n\nword");
  await page.close();
});

test("inline style toggles preserve nested emphasis and backticks remain valid code", async () => {
  const { page } = await setup("**word**");
  await select(page, 2, 6);
  await page.evaluate(() => window.studyappEditor.command("italic"));
  assert.equal((await state(page)).text, "***word***");
  await page.evaluate(() => window.studyappEditor.command("italic"));
  assert.equal((await state(page)).text, "**word**");
  await page.evaluate(() => window.studyappEditor.load({ generation: 42, text: "*word*", readonly: false }));
  await select(page, 1, 5);
  await page.evaluate(() => window.studyappEditor.command("bold"));
  assert.equal((await state(page)).text, "***word***");
  await page.evaluate(() => window.studyappEditor.command("bold"));
  assert.equal((await state(page)).text, "*word*");
  await page.evaluate(() => window.studyappEditor.load({ generation: 43, text: "a`b", readonly: false }));
  await select(page, 0, 3);
  await page.evaluate(() => window.studyappEditor.command("code"));
  assert.equal((await state(page)).text, "``a`b``");
  assert.equal(await page.locator(".md-inline-code").count(), 1);
  await page.evaluate(() => window.studyappEditor.command("code"));
  assert.equal((await state(page)).text, "a`b");
  await page.close();
});

test("inline code toggles restore text beginning or ending with backticks", async () => {
  const { page, errors } = await setup("");
  for (const text of ["`a", "a`", "``a`", "`a``"]) {
    await page.evaluate(text => window.studyappEditor.load({ generation: 42, text, readonly: false }), text);
    await select(page, 0, text.length);
    await page.evaluate(() => window.studyappEditor.command("code"));
    assert.equal(await page.locator(".md-inline-code").count(), 1);
    await page.evaluate(() => window.studyappEditor.command("code"));
    assert.equal((await state(page)).text, text);
  }
  assert.deepEqual(errors, []);
  await page.close();
});

test("formatting 140000 backtick runs does not overflow the JavaScript stack", async () => {
  const text = "a`".repeat(140000);
  const { page, errors } = await setup(text);
  await select(page, 0, text.length);
  await page.evaluate(() => window.studyappEditor.command("code"));
  const formatted = (await state(page)).text;
  assert.ok(formatted === "`` " + text + " ``", "long backtick selection must produce one valid code span");
  assert.deepEqual(errors, []);
  await page.close();
});

test("table rows with missing cells retain the header column widths", async () => {
  const text = "| A | B | C |\n|---|---|---|\n| one | two | three |\n| short |\n\nend";
  const { page } = await setup(text);
  await select(page, text.length);
  const widths = await page.locator(".md-table-row").evaluateAll(rows => rows.map(row => row.querySelector(".md-table-cell").getBoundingClientRect().width));
  assert.equal(widths.length, 3);
  assert.ok(Math.abs(widths[0] - widths[2]) < 2, JSON.stringify(widths));
  await page.close();
});

test("tables, fenced languages and unknown code remain in one editable document", async () => {
  const languages = {
    rust: "fn main() { let count = 3; }", python: "def main():\n    return True",
    javascript: "const value = true;", typescript: "const value: number = 3;",
    c: "int main() { return 0; }", cpp: "class Sample { public: int value; };",
    csharp: "class Sample { public int Value = 3; }", java: "class Sample { int value = 3; }",
    go: "package main\nfunc main() {}", html: '<div class="sample">text</div>',
    css: ".sample { color: red; }", json: '{"value": true}', yaml: "value: true",
    toml: 'name = "hello"', sql: "SELECT value FROM sample;", shell: 'echo "$HOME"'
  };
  for (const [language, code] of Object.entries(languages)) {
    const { page } = await setup("```" + language + "\n" + code + "\n```\n\nend");
    await select(page, (await state(page)).text.length);
    assert.ok(await page.locator(".tok-keyword, .tok-string, .tok-number, .tok-property, .tok-tag, .tok-type").count(), language);
    assert.ok(await page.locator(".md-code-line").count(), language);
    await page.close();
  }
  const text = "| 名称 | 数值 |\n| :--- | ---: |\n| 样本 | 42 |\n\n```unknownlang\n<b>plain code</b>\n```\n\nend";
  const { page, errors } = await setup(text);
  await select(page, text.length);
  assert.ok(await page.locator(".md-table-row").count());
  assert.equal(await page.locator(".cm-content b").count(), 0);
  assert.equal((await state(page)).text, text);
  assert.deepEqual(errors, []);
  await page.close();
});

test("Markdown never executes HTML, navigates or loads remote resources", async () => {
  const { page, errors, externalRequests } = await setup('<script>window.pwned=true</script>\n<img src="https://example.com/tracker">\n\n![image](https://example.com/image.png)\n\n[bad](javascript:alert(1))\n[link](https://example.com)\n\nend');
  await select(page, (await state(page)).text.length);
  assert.equal(await page.evaluate(() => window.pwned), undefined);
  // CM inserts inert src-less widgetBuffer images to work around browser caret bugs.
  assert.equal(await page.locator(".cm-content script, .cm-content img:not(.cm-widgetBuffer), .cm-content img[src], .cm-content iframe").count(), 0);
  assert.equal(await page.locator(".cm-content a[href]").count(), 0);
  await page.locator(".md-link").last().click({ modifiers: ["Control"] });
  assert.equal(page.url(), base);
  assert.deepEqual(externalRequests, []);
  assert.deepEqual(errors, []);
  await page.close();
});

test("multiline link titles preserve the continuous surface without invalid replacements", async () => {
  const text = '[foo](https://example.com "a\nb")\n\nend';
  const { page, errors } = await setup(text);
  await select(page, text.length);
  assert.deepEqual(errors, []);
  assert.equal((await state(page)).text, text);
  assert.equal(await page.locator('[contenteditable="true"]').count(), 1);
  await page.keyboard.insertText("!");
  assert.equal((await state(page)).text, text + "!");
  assert.deepEqual(errors, []);
  await page.close();
});

test("task keyboard activation toggles the marker without inserting text at the editor cursor", async () => {
  const { page } = await setup("- [ ] task\n\nend");
  await select(page, 14);
  const checkbox = page.getByRole("checkbox", { name: "切换任务完成状态" });
  await checkbox.focus();
  await page.keyboard.press("Enter");
  assert.equal((await state(page)).text, "- [x] task\n\nend");
  await page.keyboard.press("Space");
  assert.equal((await state(page)).text, "- [ ] task\n\nend");
  await page.close();
});

test("keyboard selection crosses paragraphs and prefix patches use the pre-transaction document", async () => {
  const { page } = await setup("one\n\ntwo\n\nthree");
  await select(page, 0);
  await page.keyboard.press("Control+Shift+End");
  const selected = (await state(page)).selection;
  assert.deepEqual(selected, { anchor: 0, head: 15 });
  await page.getByRole("button", { name: "列表", exact: true }).click();
  assert.equal((await state(page)).text, "- one\n- \n- two\n- \n- three");
  const change = await page.evaluate(() => window.messages.filter(m => m.type === "change").at(-1));
  assert.equal(change.changes.length, 5);
  assert.deepEqual(change.changes.map(c => [c.fromLine, c.fromColumn, c.toLine, c.toColumn]), [
    [1, 1, 1, 1], [2, 1, 2, 1], [3, 1, 3, 1], [4, 1, 4, 1], [5, 1, 5, 1]
  ]);
  await page.keyboard.press("Control+z");
  assert.equal((await state(page)).text, "one\n\ntwo\n\nthree");
  await page.keyboard.insertText("替换全部\n新段落");
  assert.equal((await state(page)).text, "替换全部\n新段落");
  await page.close();
});

test("new load cancels an old flush and resets history, selection and revisions", async () => {
  const { page } = await setup("old");
  await page.evaluate(() => {
    window.studyappEditor.flush({ request: 99 });
    window.studyappEditor.load({ generation: 42, text: "next", readonly: false });
  });
  await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => window.messages.filter(m => m.type === "flush").length), 0);
  assert.equal((await state(page)).readonly, false);
  assert.equal((await state(page)).revision, 0);
  await page.evaluate(() => window.studyappEditor.command("undo"));
  assert.equal((await state(page)).text, "next");
  await action(page, { type: "replace", from: 4, text: "\r\n下一行\r末尾", request: "native-smoke" });
  assert.equal((await state(page)).text, "next\n下一行\n末尾");
  const report = await page.evaluate(() => window.messages.find(m => m.type === "test"));
  assert.equal(report.request, "native-smoke");
  assert.equal(report.generation, 42);
  assert.equal(report.result.normalizedText, "next\n下一行\n末尾");
  assert.equal(report.result.revision, 1);
  await page.close();
});

test("malformed Markdown and repeated IME updates retain literal text and flush accurately", async () => {
  const raw = "# 标题\r\n\r\n未闭合 **强调 [链接](\r\n\r\n```not-a-language\r\n<script>alert('literal')</script>";
  const { page, errors } = await setup(raw);
  await select(page, 4);
  const session = await page.context().newCDPSession(page);
  await session.send("Input.imeSetComposition", { text: "中", selectionStart: 1, selectionEnd: 1 });
  await session.send("Input.imeSetComposition", { text: "中文", selectionStart: 2, selectionEnd: 2 });
  await session.send("Input.insertText", { text: "中文输入" });
  await page.evaluate(() => window.studyappEditor.flush({ request: 73 }));
  const expected = "# 标题中文输入\n\n未闭合 **强调 [链接](\n\n```not-a-language\n<script>alert('literal')</script>";
  assert.equal((await state(page)).text, expected);
  const flush = await page.evaluate(() => window.messages.find(m => m.type === "flush" && m.request === 73));
  assert.equal(flush.normalizedText, expected);
  assert.deepEqual(errors, []);
  await page.close();
});

test("flush commits composition before freezing and acknowledges the matching revision", async () => {
  const { page } = await setup("输入：");
  await select(page, 3);
  const session = await page.context().newCDPSession(page);
  await session.send("Input.imeSetComposition", { text: "中文", selectionStart: 2, selectionEnd: 2 });
  assert.equal((await state(page)).composing, true);
  await page.evaluate(() => window.studyappEditor.flush({ request: 17 }));
  await page.waitForFunction(() => window.messages.some(m => m.type === "flush" && m.request === 17));
  const snapshot = await state(page);
  const flush = await page.evaluate(() => window.messages.find(m => m.type === "flush"));
  assert.equal(snapshot.text, "输入：中文");
  assert.equal(snapshot.readonly, true);
  assert.equal(snapshot.composing, false);
  assert.deepEqual(flush, { type: "flush", generation: 41, request: 17, revision: snapshot.revision, normalizedText: snapshot.text });
  await page.keyboard.insertText("不可插入");
  assert.equal((await state(page)).text, snapshot.text);
  await page.evaluate(() => window.studyappEditor.load({ generation: 42, text: "新文档", readonly: false }));
  assert.equal((await state(page)).revision, 0);
  assert.equal((await state(page)).generation, 42);
  await page.close();
});

test("readonly, save key, and production test isolation", async () => {
  const { page } = await setup("readonly");
  await page.evaluate(() => window.studyappEditor.setReadonly(true));
  await page.locator(".cm-content").click();
  await page.keyboard.insertText("blocked");
  assert.equal((await state(page)).text, "readonly");
  await page.keyboard.press("Control+s");
  assert.ok(await page.evaluate(() => window.messages.some(m => m.type === "save" && m.generation === 41)));
  await page.close();
  const production = await setup("secure", false);
  assert.equal(await production.page.evaluate(() => typeof window.studyappEditor.test), "undefined");
  await production.page.close();
});

test("oversized UTF-8 paste is rejected atomically and editing remains available", async () => {
  const { page } = await setup("保留原文");
  await select(page, 4);
  await page.evaluate(() => {
    const clipboard = new DataTransfer();
    clipboard.setData("text/plain", "😀".repeat(10 * 1024 * 1024 / 4 + 1));
    document.querySelector(".cm-content").dispatchEvent(new ClipboardEvent("paste", { clipboardData: clipboard, bubbles: true, cancelable: true }));
  });
  assert.ok((await state(page)).text === "保留原文", "oversized paste must preserve the original document");
  assert.equal((await state(page)).revision, 0);
  assert.equal((await state(page)).readonly, false);
  assert.match(await page.locator("#editor-status").innerText(), /10 MiB/);
  assert.equal(await page.evaluate(() => window.messages.filter(m => m.type === "change" || m.type === "error").length), 0);
  await page.keyboard.insertText("！");
  assert.equal((await state(page)).text, "保留原文！");
  await page.close();
});

test("a formatting transaction over 4096 patches is refused without losing the document", async () => {
  const text = Array.from({ length: 2050 }, (_, index) => `line ${index}`).join("\n");
  const { page } = await setup(text);
  await select(page, 0, text.length);
  await page.evaluate(() => window.studyappEditor.command("bold"));
  assert.ok((await state(page)).text === text, "oversized format must preserve the original document");
  assert.equal((await state(page)).revision, 0);
  assert.equal((await state(page)).readonly, false);
  assert.match(await page.locator("#editor-status").innerText(), /4096/);
  assert.equal(await page.evaluate(() => window.messages.filter(m => m.type === "change" || m.type === "error").length), 0);
  await select(page, 0);
  await page.keyboard.insertText("!");
  assert.equal((await state(page)).text, "!" + text);
  await page.close();
});

test("raw-byte budget preserves mixed EOL widths and counts preferred inserted CRLF exactly", async () => {
  const { page } = await setup("a\r\nb\nc\rd");
  assert.equal((await state(page)).rawBytes, 8);
  assert.equal((await state(page)).utf8Bytes, 7);
  await action(page, { type: "replace", from: 1, to: 4, text: "" });
  assert.equal((await state(page)).rawBytes, 4);
  assert.equal((await state(page)).text, "ac\nd");
  await action(page, { type: "replace", from: 1, text: "\n" });
  assert.equal((await state(page)).rawBytes, 6);
  assert.equal((await state(page)).utf8Bytes, 5);
  await page.close();
});

test("a valid 10 MiB CRLF note can shrink and edit without exceeding its raw byte budget", async () => {
  const { page } = await setup("");
  const report = await page.evaluate(async () => {
    const limit = 10 * 1024 * 1024;
    window.studyappEditor.load({ generation: 42, text: "a".repeat(limit - 2) + "\r\n", readonly: false });
    const before = await window.studyappEditor.test({ type: "snapshot" });
    await window.studyappEditor.test({ type: "replace", from: before.text.length, text: "\n" });
    const rejected = await window.studyappEditor.test({ type: "snapshot" });
    await window.studyappEditor.test({ type: "replace", from: 0, to: 2, text: "" });
    await window.studyappEditor.test({ type: "replace", from: 0, text: "\n" });
    const after = await window.studyappEditor.test({ type: "snapshot" });
    return { beforeBytes: before.rawBytes, beforeNormalized: before.utf8Bytes, rejectedRevision: rejected.revision,
      afterBytes: after.rawBytes, afterRevision: after.revision, readonly: after.readonly };
  });
  assert.deepEqual(report, { beforeBytes: 10 * 1024 * 1024, beforeNormalized: 10 * 1024 * 1024 - 1,
    rejectedRevision: 0, afterBytes: 10 * 1024 * 1024, afterRevision: 2, readonly: false });
  await page.close();
});

test("250 KiB document keeps viewport DOM bounded during 200 edits", async () => {
  const text = Array.from({ length: 3500 }, (_, i) => `## 标题 ${i}\n\nContinuous Markdown paragraph ${i} has **strong text**, a [link](https://example.com), 中文与 😀.\n\n`).join("");
  assert.ok(Buffer.byteLength(text) > 250 * 1024);
  const { page } = await setup(text);
  const report = await action(page, { type: "performance", edits: 200 });
  assert.equal(report.edits, 200);
  assert.ok(report.domNodes < 2500, JSON.stringify(report));
  assert.ok(report.renderedLines < 200, JSON.stringify(report));
  assert.ok(report.p95Ms < 50, JSON.stringify(report));
  const snapshot = await state(page);
  assert.equal(snapshot.text, "x".repeat(200) + text);
  assert.equal(snapshot.revision, 200);
  await mkdir(new URL("../test-results/", import.meta.url), { recursive: true });
  await writeFile(new URL("../test-results/performance.json", import.meta.url), JSON.stringify({ bytes: Buffer.byteLength(text), ...report }, null, 2));
  console.log("PERFORMANCE", JSON.stringify(report));
  await page.screenshot({ path: fileURLToPath(new URL("../test-results/continuous-editor.png", import.meta.url)) });
  await page.close();
});
