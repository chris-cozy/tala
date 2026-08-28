import { useEffect, useRef, useState } from "react";
import {
  EditorContent,
  useEditor,
  type JSONContent,
  type Editor,
} from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import Mathematics from "@tiptap/extension-mathematics";
import { TextStyle, Color } from "@tiptap/extension-text-style";
import Highlight from "@tiptap/extension-highlight";
import Subscript from "@tiptap/extension-subscript";
import Superscript from "@tiptap/extension-superscript";
import {
  Bold,
  Italic,
  Underline,
  List,
  ListOrdered,
  Link,
  ImagePlus,
  Code,
  Eraser,
  Undo2,
  Redo2,
  Sigma,
  Subscript as SubIcon,
  Superscript as SuperIcon,
  Highlighter,
  Palette,
} from "lucide-react";
import { diffChars } from "diff";
import { useTala } from "../lib/context";
import { mediaUrl, openExternal, pickFile, rpc } from "../lib/api";
import { IconButton } from "./ui";
import "katex/dist/katex.min.css";

export type Doc = JSONContent;
export const emptyDoc = (): Doc => ({
  type: "doc",
  content: [{ type: "paragraph" }],
});
export function textDoc(text: string): Doc {
  return {
    type: "doc",
    content: [
      { type: "paragraph", content: text ? [{ type: "text", text }] : [] },
    ],
  };
}
export function extractText(doc: Doc): string {
  function walk(node: Doc): string {
    let text = node.text ?? "";
    if (node.type?.includes("Math")) text += String(node.attrs?.latex ?? "");
    if (node.type === "image") text += String(node.attrs?.alt ?? "");
    text += (node.content ?? []).map(walk).join("");
    if (
      [
        "paragraph",
        "heading",
        "hardBreak",
        "blockMath",
        "listItem",
        "codeBlock",
      ].includes(node.type ?? "")
    )
      text += "\n";
    return text;
  }
  return walk(doc).trim();
}
export function hasContent(doc: Doc): boolean {
  return (
    !!doc.text?.trim() ||
    !!doc.attrs?.mediaId ||
    !!doc.attrs?.latex ||
    !!doc.content?.some(hasContent)
  );
}
/** Hydrate a copy with local asset URLs without changing the canonical saved document. */
export function displayDoc(value: unknown, directory: string): Doc {
  const doc = JSON.parse(JSON.stringify(value)) as Doc;
  function walk(node: Doc) {
    if (node.type === "image")
      node.attrs = {
        ...node.attrs,
        src: mediaUrl(directory, node.attrs?.mediaId),
      };
    node.content?.forEach(walk);
  }
  walk(doc);
  return doc;
}
const LocalImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      mediaId: {
        default: null,
        parseHTML: (el) => el.getAttribute("data-media-id"),
        renderHTML: (attrs) => ({ "data-media-id": attrs.mediaId }),
      },
    };
  },
});
// Editor and read-only previews share a schema, matching native content validation.
function extensions(
  onMath?: (latex: string, pos: number, block: boolean) => void,
) {
  return [
    StarterKit.configure({ link: { openOnClick: false, autolink: false } }),
    LocalImage.configure({ allowBase64: false }),
    TextStyle,
    Color,
    Highlight.configure({ multicolor: true }),
    Subscript,
    Superscript,
    Mathematics.configure({
      katexOptions: { throwOnError: false, trust: false, strict: false },
      inlineOptions: {
        onClick: (node, pos) => onMath?.(node.attrs.latex, pos, false),
      },
      blockOptions: {
        onClick: (node, pos) => onMath?.(node.attrs.latex, pos, true),
      },
    }),
  ];
}
export function ContentRender({
  value,
  directory,
  className = "",
}: {
  value: unknown;
  directory: string;
  className?: string;
}) {
  const editor = useEditor({
    extensions: extensions(),
    editable: false,
    content: displayDoc(value, directory),
    editorProps: {
      attributes: { class: "rich-document", "aria-label": "Card content" },
      handleClick: (_view, _pos, event) => {
        const link = (event.target as HTMLElement).closest("a");
        if (link) {
          event.preventDefault();
          void openExternal(link.href);
          return true;
        }
        return false;
      },
    },
  });
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    editor.commands.setContent(displayDoc(value, directory), {
      emitUpdate: false,
    });
  }, [editor, value, directory]);
  return (
    <div
      className={`content-render ${className}`}
      onErrorCapture={(e) => {
        if (e.target instanceof HTMLImageElement)
          e.target.alt = "Image missing — attach it again in the editor";
      }}
    >
      <EditorContent editor={editor} />
    </div>
  );
}
export function RichField({
  value,
  onChange,
  label,
}: {
  value: Doc;
  onChange: (value: Doc) => void;
  label: string;
}) {
  const { data, run, ask, notify } = useTala();
  const editorRef = useRef<Editor | null>(null);
  const [, setVersion] = useState(0);
  const [uploading, setUploading] = useState(false);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  async function attachBytes(file: File) {
    setUploading(true);
    const media = await run(async () => {
      if (file.size > 20 * 1024 * 1024)
        throw new Error("Images must be smaller than 20 MB.");
      return rpc({
        action: "attach_bytes",
        payload: {
          bytes: Array.from(new Uint8Array(await file.arrayBuffer())),
        },
      });
    });
    if (media)
      editorRef.current
        ?.chain()
        .focus()
        .setImage({
          src: mediaUrl(data.mediaDir, media)!,
          alt: file.name,
          ...{ mediaId: media },
        })
        .run();
    setUploading(false);
  }
  const editor = useEditor({
    extensions: extensions(async (latex, pos, block) => {
      const next = await ask("Edit equation", "LaTeX", latex);
      if (next != null)
        editorRef.current
          ?.chain()
          .focus()
          .setNodeSelection(pos)
          .insertContent({
            type: block ? "blockMath" : "inlineMath",
            attrs: { latex: next },
          })
          .run();
    }),
    content: displayDoc(value, data.mediaDir),
    onUpdate: ({ editor }) => onChangeRef.current(editor.getJSON()),
    onTransaction: () => setVersion((v) => v + 1),
    editorProps: {
      attributes: {
        class: "rich-document",
        role: "textbox",
        "aria-label": label,
        "aria-multiline": "true",
        "data-placeholder":
          label === "Front"
            ? "What would you like to remember?"
            : "The answer, in your own words…",
      },
      handlePaste: (_view, event) => {
        const file = Array.from(event.clipboardData?.files ?? []).find((f) =>
          f.type.startsWith("image/"),
        );
        if (file) {
          event.preventDefault();
          void attachBytes(file);
          return true;
        }
        return false;
      },
      handleDrop: (_view, event) => {
        const file = Array.from(event.dataTransfer?.files ?? []).find((f) =>
          f.type.startsWith("image/"),
        );
        if (file) {
          event.preventDefault();
          void attachBytes(file);
          return true;
        }
        return false;
      },
    },
  });
  editorRef.current = editor;
  useEffect(() => {
    if (
      editor &&
      !editor.isDestroyed &&
      JSON.stringify(editor.getJSON()) !==
        JSON.stringify(displayDoc(value, data.mediaDir))
    )
      editor.commands.setContent(displayDoc(value, data.mediaDir), {
        emitUpdate: false,
      });
  }, [value, data.mediaDir, editor]);
  async function image() {
    setUploading(true);
    const media = await run(async () => {
      const selected = await pickFile("image");
      return selected
        ? rpc({ action: "attach_file", payload: { token: selected.token } })
        : null;
    });
    if (media)
      editor
        ?.chain()
        .focus()
        .insertContent({
          type: "image",
          attrs: {
            src: mediaUrl(data.mediaDir, media),
            mediaId: media,
            alt: "",
          },
        })
        .run();
    setUploading(false);
  }
  async function link() {
    const url = await ask(
      "Add a link",
      "Web or email address",
      editor?.getAttributes("link").href ?? "https://",
    );
    if (url == null) return;
    if (!url) {
      editor?.chain().focus().unsetLink().run();
      return;
    }
    if (!/^(https?:\/\/|mailto:)/i.test(url)) {
      notify("Use an http, https, or mailto link.", true);
      return;
    }
    editor?.chain().focus().setLink({ href: url }).run();
  }
  async function math(block: boolean) {
    const latex = await ask(
      block ? "Add a display equation" : "Add an inline equation",
      "LaTeX",
      "",
    );
    if (latex)
      editor
        ?.chain()
        .focus()
        .insertContent({
          type: block ? "blockMath" : "inlineMath",
          attrs: { latex },
        })
        .run();
  }
  const tools = [
    {
      label: "Bold",
      icon: Bold,
      active: editor?.isActive("bold"),
      action: () => editor?.chain().focus().toggleBold().run(),
    },
    {
      label: "Italic",
      icon: Italic,
      active: editor?.isActive("italic"),
      action: () => editor?.chain().focus().toggleItalic().run(),
    },
    {
      label: "Underline",
      icon: Underline,
      active: editor?.isActive("underline"),
      action: () => editor?.chain().focus().toggleUnderline().run(),
    },
    {
      label: "Inline code",
      icon: Code,
      active: editor?.isActive("code"),
      action: () => editor?.chain().focus().toggleCode().run(),
    },
    {
      label: "Bullet list",
      icon: List,
      active: editor?.isActive("bulletList"),
      action: () => editor?.chain().focus().toggleBulletList().run(),
    },
    {
      label: "Numbered list",
      icon: ListOrdered,
      active: editor?.isActive("orderedList"),
      action: () => editor?.chain().focus().toggleOrderedList().run(),
    },
    {
      label: "Link",
      icon: Link,
      active: editor?.isActive("link"),
      action: () => void link(),
    },
    { label: "Attach image", icon: ImagePlus, action: () => void image() },
    { label: "Inline equation", icon: Sigma, action: () => void math(false) },
    { label: "Display equation", icon: Sigma, action: () => void math(true) },
    {
      label: "Superscript",
      icon: SuperIcon,
      active: editor?.isActive("superscript"),
      action: () => editor?.chain().focus().toggleSuperscript().run(),
    },
    {
      label: "Subscript",
      icon: SubIcon,
      active: editor?.isActive("subscript"),
      action: () => editor?.chain().focus().toggleSubscript().run(),
    },
  ];
  return (
    <div className={`rich-field ${uploading ? "uploading" : ""}`}>
      <div
        className="format-toolbar"
        role="toolbar"
        aria-label={`${label} formatting`}
      >
        {tools.map(({ label: tip, icon: Icon, active, action }) => (
          <IconButton
            key={tip}
            label={tip}
            aria-pressed={active}
            onClick={action}
            disabled={uploading}
          >
            <Icon size={16} />
            {tip === "Display equation" && (
              <span className="block-math-marker">▭</span>
            )}
          </IconButton>
        ))}
        <label className="color-control" title="Text color">
          <Palette size={16} />
          <input
            type="color"
            aria-label={`${label} text color`}
            defaultValue="#aa91ff"
            onChange={(e) =>
              editor?.chain().focus().setColor(e.target.value).run()
            }
          />
        </label>
        <label className="color-control" title="Highlight color">
          <Highlighter size={16} />
          <input
            type="color"
            aria-label={`${label} highlight color`}
            defaultValue="#554b80"
            onChange={(e) =>
              editor
                ?.chain()
                .focus()
                .setHighlight({ color: e.target.value })
                .run()
            }
          />
        </label>
        <span className="toolbar-separator" />
        <IconButton
          label="Clear formatting"
          onClick={() =>
            editor?.chain().focus().unsetAllMarks().clearNodes().run()
          }
        >
          <Eraser size={16} />
        </IconButton>
        <IconButton
          label={`Undo ${label} edit`}
          onClick={() => editor?.chain().focus().undo().run()}
        >
          <Undo2 size={15} />
        </IconButton>
        <IconButton
          label={`Redo ${label} edit`}
          onClick={() => editor?.chain().focus().redo().run()}
        >
          <Redo2 size={15} />
        </IconButton>
      </div>
      <EditorContent editor={editor} />
      {uploading && <div className="field-status">Adding image…</div>}
    </div>
  );
}
export function AnswerComparison({
  answer,
  expected,
}: {
  answer: string;
  expected: string;
}) {
  const actual = answer.normalize("NFC").trim().replace(/\s+/g, " ");
  const correct = expected.normalize("NFC").trim().replace(/\s+/g, " ");
  return (
    <div className="answer-comparison">
      <span className="eyebrow">YOUR ANSWER</span>
      <p>{answer || <em>No answer entered</em>}</p>
      <div className="diff-line" aria-label="Answer differences">
        {diffChars(actual, correct).map((part, i) => (
          <span
            key={i}
            className={
              part.added ? "diff-added" : part.removed ? "diff-removed" : ""
            }
          >
            {part.value}
          </span>
        ))}
      </div>
      <small>Use your own judgment when grading recall.</small>
    </div>
  );
}
