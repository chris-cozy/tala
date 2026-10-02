import {
  useEffect,
  type RefObject,
  type MouseEvent,
  type KeyboardEvent,
} from "react";
import {
  Node,
  mergeAttributes,
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type NodeViewProps,
} from "@tiptap/react";
import { useState, useRef } from "react";
import { RotateCcw, Volume2, Trash2 } from "lucide-react";
import { closeHistory } from "@tiptap/pm/history";

function AudioView({ node, editor, deleteNode }: NodeViewProps) {
  const audio = useRef<HTMLAudioElement>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    const player = audio.current;
    const stop = () => player?.pause();
    const observer = new MutationObserver(() => {
      if (
        [...document.querySelectorAll('[role="dialog"]')].some(
          (dialog) => !dialog.contains(player),
        )
      )
        stop();
    });
    observer.observe(document.body, { childList: true, subtree: true });
    window.addEventListener("blur", stop);
    window.addEventListener("tala:stop-audio", stop);
    return () => {
      stop();
      observer.disconnect();
      window.removeEventListener("blur", stop);
      window.removeEventListener("tala:stop-audio", stop);
    };
  }, []);
  function manual() {
    window.dispatchEvent(new Event("tala:manual-audio"));
  }
  return (
    <NodeViewWrapper
      className="audio-clip"
      contentEditable={false}
      onClick={(event: MouseEvent<HTMLDivElement>) => event.stopPropagation()}
      onPointerDown={manual}
      onKeyDown={(event: KeyboardEvent<HTMLDivElement>) => {
        manual();
        event.stopPropagation();
      }}
    >
      <div className="audio-label">
        <Volume2 size={15} />
        <span>{node.attrs.label || "Audio"}</span>
      </div>
      <div className="audio-controls">
        <audio
          ref={audio}
          className="tala-audio"
          src={node.attrs.src ?? undefined}
          controls
          preload="metadata"
          aria-label={node.attrs.label || "Card audio"}
          onPlay={() => {
            document
              .querySelectorAll<HTMLAudioElement>(".tala-audio")
              .forEach((other) => {
                if (other !== audio.current) other.pause();
              });
            setError("");
          }}
          onError={() =>
            setError(
              "Audio unavailable. Reattach it in the editor or restore a backup.",
            )
          }
        />
        <button
          type="button"
          className="icon-button"
          aria-label={`Replay ${node.attrs.label || "audio"}`}
          onClick={() => {
            const player = audio.current;
            if (!player) return;
            manual();
            player.currentTime = 0;
            void player
              .play()
              .catch(() =>
                setError("Audio could not play. Try the play control."),
              );
          }}
        >
          <RotateCcw size={16} />
        </button>
        {editor.isEditable && (
          <button
            type="button"
            className="icon-button"
            aria-label={`Remove ${node.attrs.label || "audio"}`}
            onClick={() => {
              audio.current?.pause();
              editor.view.dispatch(closeHistory(editor.state.tr));
              deleteNode();
              editor.commands.focus();
            }}
          >
            <Trash2 size={16} />
          </button>
        )}
      </div>
      {error && (
        <small role="alert" className="audio-error">
          {error}
        </small>
      )}
    </NodeViewWrapper>
  );
}

export const LocalAudio = Node.create({
  name: "audio",
  group: "block",
  atom: true,
  draggable: true,
  addAttributes() {
    return {
      mediaId: {
        default: null,
        parseHTML: (el) => el.getAttribute("data-media-id"),
      },
      label: {
        default: "",
        parseHTML: (el) => el.getAttribute("data-label") || "",
      },
      src: { default: null },
    };
  },
  // Arbitrary pasted <audio src> is never treated as a local attachment.
  parseHTML() {
    return [{ tag: "div[data-tala-audio][data-media-id]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(
        {
          "data-tala-audio": "",
          "data-media-id": HTMLAttributes.mediaId,
          "data-label": HTMLAttributes.label,
        },
        {},
      ),
    ];
  },
  addNodeView() {
    return ReactNodeViewRenderer(AudioView);
  },
});

export function stopAudio() {
  window.dispatchEvent(new Event("tala:stop-audio"));
}

/** One visible-side queue, cancelled whenever its view loses ownership of playback. */
export function useAudioPlayback(
  host: RefObject<HTMLDivElement | null>,
  key: string | undefined,
  autoplay: boolean,
) {
  const [error, setError] = useState("");
  useEffect(() => {
    setError("");
    const controller = new AbortController();
    const stop = () => {
      controller.abort();
      host.current
        ?.querySelectorAll<HTMLAudioElement>("audio")
        .forEach((audio) => audio.pause());
    };
    const cancelQueue = () => controller.abort();
    const visibility = () => {
      if (document.visibilityState !== "visible") stop();
    };
    window.addEventListener("blur", stop);
    window.addEventListener("tala:manual-audio", cancelQueue);
    window.addEventListener("tala:stop-audio", stop);
    document.addEventListener("visibilitychange", visibility);
    const observer = new MutationObserver(() => {
      const dialogs = [...document.querySelectorAll('[role="dialog"]')];
      if (dialogs.some((dialog) => !dialog.contains(host.current))) stop();
    });
    observer.observe(document.body, { childList: true, subtree: true });
    const timer = window.setTimeout(async () => {
      if (!autoplay || !key || controller.signal.aborted) return;
      const players = [
        ...(host.current?.querySelectorAll<HTMLAudioElement>("audio") ?? []),
      ];
      for (const player of players) {
        if (controller.signal.aborted) break;
        player.currentTime = 0;
        const ended = new Promise<void>((resolve) => {
          const finish = () => {
            player.removeEventListener("ended", finish);
            player.removeEventListener("error", finish);
            controller.signal.removeEventListener("abort", finish);
            resolve();
          };
          player.addEventListener("ended", finish, { once: true });
          player.addEventListener("error", finish, { once: true });
          controller.signal.addEventListener("abort", finish, { once: true });
        });
        try {
          await player.play();
        } catch {
          if (!controller.signal.aborted) {
            setError(
              "Auto-play was unavailable. Use the play or replay controls.",
            );
            controller.abort();
          }
          break;
        }
        await ended;
      }
    }, 50);
    return () => {
      clearTimeout(timer);
      stop();
      observer.disconnect();
      window.removeEventListener("blur", stop);
      window.removeEventListener("tala:manual-audio", cancelQueue);
      window.removeEventListener("tala:stop-audio", stop);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, [host, key, autoplay]);
  return error;
}
