import { markdown } from "@codemirror/lang-markdown";
import { syntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, Decoration, ViewPlugin, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import "./editor.css";

let activeDocumentId = null;
let editorView = null;
let loadingDocument = false;
let queuedDocument = null;

function post(kind, markdownText = editorView?.state.doc.toString() ?? "") {
  if (!activeDocumentId || !window.ipc) return;
  window.ipc.postMessage(
    JSON.stringify({ kind, page_id: activeDocumentId, markdown: markdownText }),
  );
}

function livePreviewDecorations(view) {
  const decorations = [];
  const selection = view.state.selection.main;
  const activeLines = new Set([
    view.state.doc.lineAt(selection.anchor).number,
    view.state.doc.lineAt(selection.head).number,
  ]);

  for (const range of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from: range.from,
      to: range.to,
      enter(node) {
        const line = view.state.doc.lineAt(node.from);
        if (node.name.startsWith("ATXHeading")) {
          const level = node.name.replace("ATXHeading", "");
          decorations.push(
            Decoration.line({ class: `cm-live-heading cm-live-h${level}` }).range(line.from),
          );
        } else if (node.name === "SetextHeading1") {
          decorations.push(Decoration.line({ class: "cm-live-heading cm-live-h1" }).range(line.from));
        } else if (node.name === "SetextHeading2") {
          decorations.push(Decoration.line({ class: "cm-live-heading cm-live-h2" }).range(line.from));
        }

        if (activeLines.has(line.number)) return;
        if ([
          "HeaderMark",
          "QuoteMark",
          "ListMark",
          "TaskMarker",
          "EmphasisMark",
          "CodeMark",
          "LinkMark",
          "ImageMark",
        ].includes(node.name)) {
          decorations.push(Decoration.replace({}).range(node.from, node.to));
        }
      },
    });
  }

  return Decoration.set(decorations.sort((left, right) => left.from - right.from || left.to - right.to));
}

const livePreview = ViewPlugin.fromClass(
  class {
    constructor(view) {
      this.decorations = livePreviewDecorations(view);
    }

    update(update) {
      if (update.docChanged || update.selectionSet || update.viewportChanged) {
        this.decorations = livePreviewDecorations(update.view);
      }
    }
  },
  { decorations: (plugin) => plugin.decorations },
);

function createEditor(markdownText) {
  const parent = document.getElementById("editor");
  const state = EditorState.create({
    doc: markdownText,
    extensions: [
      basicSetup,
      markdown(),
      livePreview,
      EditorView.lineWrapping,
      keymap.of([
        {
          key: "Mod-s",
          run(view) {
            post("save", view.state.doc.toString());
            return true;
          },
        },
        {
          key: "Mod-b",
          run(view) {
            return wrapSelection(view, "**");
          },
        },
        {
          key: "Mod-i",
          run(view) {
            return wrapSelection(view, "*");
          },
        },
      ]),
      EditorView.updateListener.of((update) => {
        if (!loadingDocument && update.docChanged) post("changed", update.state.doc.toString());
      }),
      EditorView.theme(
        {
          "&": { height: "100%", backgroundColor: "transparent", color: "var(--writer-text)" },
          ".cm-scroller": {
            overflow: "auto",
            fontFamily: "var(--writer-font)",
            lineHeight: "1.72",
          },
          ".cm-content": {
            maxWidth: "780px",
            minHeight: "100%",
            margin: "0 auto",
            padding: "28px 48px 100px",
            caretColor: "var(--writer-accent)",
          },
          ".cm-line": { padding: "1px 0" },
          ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--writer-accent) 5%, transparent)" },
          ".cm-gutters": { display: "none" },
          ".cm-focused": { outline: "none" },
          ".cm-selectionBackground, ::selection": {
            backgroundColor: "color-mix(in srgb, var(--writer-accent) 22%, transparent) !important",
          },
        },
        { dark: false },
      ),
    ],
  });
  editorView = new EditorView({ state, parent });
  editorView.contentDOM.addEventListener("blur", () => post("save"));
  if (queuedDocument) loadDocument(queuedDocument);
}

function wrapSelection(view, marker) {
  const selection = view.state.selection.main;
  const text = view.state.sliceDoc(selection.from, selection.to);
  view.dispatch({
    changes: {
      from: selection.from,
      to: selection.to,
      insert: `${marker}${text || "文字"}${marker}`,
    },
    selection: {
      anchor: selection.from + marker.length,
      head: selection.from + marker.length + (text || "文字").length,
    },
    scrollIntoView: true,
  });
  view.focus();
  return true;
}

function loadDocument(payload) {
  if (!payload || typeof payload.id !== "string" || typeof payload.markdown !== "string") return;
  if (!editorView) {
    queuedDocument = payload;
    return;
  }
  queuedDocument = null;
  loadingDocument = true;
  activeDocumentId = payload.id;
  const current = editorView.state.doc.toString();
  if (current !== payload.markdown) {
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: payload.markdown },
      selection: { anchor: 0 },
    });
  }
  loadingDocument = false;
}

window.personalDesk = {
  loadDocument,
  setTheme(theme) {
    if (!theme) return;
    const toRgb = (value) => `rgb(${value.map((part) => Math.max(0, Math.min(255, Number(part) || 0))).join(" ")})`;
    for (const name of ["accent", "surface", "text", "muted"]) {
      if (Array.isArray(theme[name]) && theme[name].length === 3) {
        document.documentElement.style.setProperty(`--writer-${name}`, toRgb(theme[name]));
      }
    }
  },
  flush() {
    post("save");
  },
  insertFormat(marker) {
    if (!editorView || typeof marker !== "string") return;
    const selection = editorView.state.selection.main;
    editorView.dispatch({
      changes: { from: selection.from, to: selection.to, insert: marker },
      selection: { anchor: selection.from + marker.length },
      scrollIntoView: true,
    });
    editorView.focus();
  },
};

createEditor("");
window.ipc?.postMessage(JSON.stringify({ kind: "ready" }));
