import * as monaco from "monaco-editor/editor";
import "monaco-editor/features/register.all";
import EditorWorker from "monaco-editor/editor/editor.worker.js?worker";
import "monaco-editor/languages/definitions/typescript/register";
import "monaco-editor/languages/definitions/javascript/register";
import "monaco-editor/languages/definitions/css/register";
import "monaco-editor/languages/definitions/html/register";
import "monaco-editor/languages/definitions/markdown/register";
import "monaco-editor/languages/definitions/rust/register";
import "monaco-editor/languages/definitions/csharp/register";
import "monaco-editor/languages/definitions/python/register";
import "monaco-editor/languages/definitions/yaml/register";
import "monaco-editor/languages/definitions/xml/register";
import "monaco-editor/languages/definitions/sql/register";
import "monaco-editor/languages/definitions/powershell/register";
import "monaco-editor/languages/definitions/shell/register";
import { cssColorToHex } from "./view";

self.MonacoEnvironment = { getWorker: () => new EditorWorker() };

const lightScheme = window.matchMedia("(prefers-color-scheme: light)");

function applyTheme() {
  const style = getComputedStyle(document.documentElement);
  const color = (name: string) => cssColorToHex(style.getPropertyValue(name));
  const light = lightScheme.matches;
  const name = light ? "ao-light" : "ao-dark";
  monaco.editor.defineTheme(name, {
    base: light ? "vs" : "vs-dark",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": color("--color-terminal"),
      "editor.foreground": color("--color-text"),
      "editorLineNumber.foreground": color("--color-text-muted"),
      "editorLineNumber.activeForeground": color("--color-text"),
      "editor.selectionBackground": color("--color-selected"),
      "editor.lineHighlightBackground": color("--color-panel"),
      "editorCursor.foreground": color("--color-accent"),
      "editorWidget.background": color("--color-surface"),
      "editorWidget.border": color("--color-border"),
      "focusBorder": color("--color-accent"),
      "diffEditor.insertedLineBackground": color("--diff-added-background"),
      "diffEditor.removedLineBackground": color("--diff-removed-background"),
      "diffEditor.insertedTextBackground": color("--diff-added-background"),
      "diffEditor.removedTextBackground": color("--diff-removed-background"),
      "diffEditorGutter.insertedLineBackground": color("--diff-added-background"),
      "diffEditorGutter.removedLineBackground": color("--diff-removed-background"),
      "diffEditor.border": color("--color-border"),
    },
  });
  monaco.editor.setTheme(name);
}

applyTheme();
lightScheme.addEventListener("change", applyTheme);

export const sharedOptions = {
  fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim(),
  fontSize: 12,
  lineHeight: 18,
  minimap: { enabled: false },
  links: false,
  automaticLayout: true,
  scrollBeyondLastLine: false,
  padding: { top: 8 },
} satisfies monaco.editor.IEditorOptions;

export { monaco };
