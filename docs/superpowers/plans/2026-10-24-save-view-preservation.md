# Save view preservation implementation plan

**Goal:** Save a note without moving the reader/editor to the document start or losing its selection and undo history, while keeping input frozen until the durable retirement receipt.

**Design:** Keep the existing CodeMirror state for the same document. An unchanged save only changes readonly state; image adoption applies bounded character edits to that state, maps selection/history and the viewport anchor, and starts a fresh host protocol generation. A successful save binds the existing editor to the published version rather than loading its text again. Reopening that same version preserves its state. Folder metadata waits until the exact-revision transaction has settled.

The existing user authorization covers implementation without another approval pause. No source, draft, history, image, or other file is deleted, and no commit or release is created here.

1. Add a browser regression with a long document, edits at two image references, a selection between them, a scrolled viewport, and an existing undo action. Verify rebasing maps the selection and viewport, keeps the editor readonly, and leaves undo functional after reopening.
2. Add bounded native character diff serialization in `editor_protocol.rs`, using the already present `similar` package. Preserve Unicode UTF-16 offsets and exact raw host text. Test that applying serialized edits reconstructs the normalized target and leaves unchanged spans untouched.
3. Add a `rebase` browser bridge operation that validates the old generation/revision and target text, uses a single non-history CodeMirror transaction with mapped scroll snapshot, and resets byte-budget metadata for raw newline preservation.
4. Update `MarkdownEditor`, media adoption and the save acknowledgement path to reuse same-document state. Keep load/reset for genuine document switches and keep the existing freeze barrier through final acknowledgement.
5. Run TypeScript check, editor build and targeted browser tests; root runs the GPUI/native and workspace verification serially. Record exact results and remaining limitations in the final review report.
