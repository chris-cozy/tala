import { useCallback, useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Check,
  Clock3,
  Eye,
  Keyboard,
  RotateCcw,
  Undo2,
  Minus,
  ChevronsRight,
  Pencil,
  Info,
  Pause,
  Archive,
} from "lucide-react";
import { useTala } from "../lib/context";
import { rpc } from "../lib/api";
import { isTyping, number, relativeDue } from "../lib/format";
import {
  AnswerComparison,
  ContentRender,
  extractText,
  type Doc,
} from "../components/RichContent";
import {
  Button,
  Empty,
  ErrorPanel,
  Loading,
  Menu,
  Progress,
} from "../components/ui";
import { CardDetails } from "../components/CardDetails";

/** Monotonic timing excludes time spent with the app unfocused; grades stay manual. */
function useAnswerTimer(key: string) {
  const elapsed = useRef(0);
  const started = useRef<number | null>(null);
  useEffect(() => {
    elapsed.current = 0;
    started.current = document.hasFocus() ? performance.now() : null;
    const pause = () => {
      if (started.current != null) {
        elapsed.current += performance.now() - started.current;
        started.current = null;
      }
    };
    const resume = () => {
      if (document.visibilityState === "visible" && started.current == null)
        started.current = performance.now();
    };
    const visibility = () =>
      document.visibilityState === "visible" ? resume() : pause();
    window.addEventListener("blur", pause);
    window.addEventListener("focus", resume);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      window.removeEventListener("blur", pause);
      window.removeEventListener("focus", resume);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, [key]);
  return () =>
    Math.round(
      elapsed.current +
        (started.current == null ? 0 : performance.now() - started.current),
    );
}
export default function StudyPage() {
  const { data, run, navigate } = useTala();
  const client = useQueryClient();
  const query = useQuery({
    queryKey: ["study"],
    queryFn: () => rpc({ action: "study" }),
    refetchInterval: (query) =>
      query.state.data?.card || query.state.data?.finished ? false : 1000,
  });
  const view = query.data;
  const card = view?.card;
  const [revealed, setRevealed] = useState(false);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [details, setDetails] = useState(false);
  const flight = useRef(false);
  const surface = useRef<HTMLDivElement>(null);
  const timer = useAnswerTimer(`${card?.id}-${card?.revision}`);
  useEffect(() => {
    setRevealed(false);
    setTyped("");
  }, [card?.id, card?.revision]);
  const reveal = useCallback(() => {
    if (card && !revealed) {
      setRevealed(true);
      surface.current?.focus({ preventScroll: true });
    }
  }, [card, revealed]);
  async function grade(value: number) {
    if (!view || !card || !revealed || flight.current) return;
    flight.current = true;
    setBusy(true);
    const result = await run(() =>
      rpc({
        action: "review",
        payload: {
          sessionId: view.session.id,
          cardId: card.id,
          revision: card.revision,
          operationId: crypto.randomUUID(),
          grade: value,
          durationMs: timer(),
        },
      }),
    );
    if (result) client.setQueryData(["study"], result);
    else await query.refetch();
    setBusy(false);
    flight.current = false;
  }
  async function undo() {
    const result = await run(() => rpc({ action: "undo" }), "Review undone");
    if (result) {
      setRevealed(false);
      await query.refetch();
    }
  }
  async function manage(action: "bury" | "suspend") {
    if (!card) return;
    await run(
      () =>
        rpc({
          action: "bulk",
          payload: { ids: [card.id], action, value: null },
        }),
      action === "bury" ? "Card buried until tomorrow" : "Card suspended",
    );
    await query.refetch();
  }
  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (
        isTyping(event.target) ||
        (event.target instanceof HTMLElement &&
          event.target.closest(".audio-clip")) ||
        document.querySelector('[role="dialog"]')
      )
        return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z") {
        event.preventDefault();
        if (data.undoLabel) void undo();
        return;
      }
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.code === "Space" && !revealed) {
        event.preventDefault();
        reveal();
      }
      if (revealed && /^[1-4]$/.test(event.key)) {
        event.preventDefault();
        void grade(Number(event.key));
      }
      if (event.key.toLowerCase() === "b") void manage("bury");
      if (event.key.toLowerCase() === "s") void manage("suspend");
      if (event.key.toLowerCase() === "e" && card)
        void navigate({ page: "editor", cardId: card.id });
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  });
  if (query.isPending) return <Loading />;
  if (query.error)
    return (
      <ErrorPanel
        message={(query.error as Error).message}
        retry={() => void query.refetch()}
      />
    );
  if (!view)
    return (
      <Empty
        title="Ready when you are"
        action={
          <Button
            variant="primary"
            onClick={() => void navigate({ page: "today" })}
          >
            Back to Today
          </Button>
        }
      >
        Start a session from Today or one of your decks.
      </Empty>
    );
  const { session } = view;
  const deckName =
    data.decks.find((d) => d.id === session.deckId)?.name ?? "Today’s study";
  const done = session.completed.length + session.skipped.length;
  return (
    <div className="study-page">
      <header className="study-header">
        <Button
          variant="ghost"
          onClick={() => void navigate({ page: "today" })}
        >
          <ArrowLeft size={17} />
          Leave session
        </Button>
        <div>
          <span>{deckName}</span>
          <small>One thought at a time</small>
        </div>
        <Button
          variant="ghost"
          disabled={!data.undoLabel || busy}
          onClick={undo}
        >
          <Undo2 size={16} />
          Undo
        </Button>
      </header>
      <div className="study-progress">
        <Progress
          value={done / (session.cardIds.length || 1)}
          label="Current session progress"
        />
        <span>
          {number(done)} <i>/ {number(session.cardIds.length)}</i>
        </span>
      </div>
      {card ? (
        <>
          <div className={`card-stack ${busy ? "advancing" : ""}`}>
            <div className="stack-layer layer-three" aria-hidden="true" />
            <div className="stack-layer layer-two" aria-hidden="true" />
            <div className="stack-layer layer-one" aria-hidden="true" />
            <div
              key={`${card.id}-${card.revision}`}
              className={`study-card ${revealed ? "revealed" : ""}`}
              ref={surface}
              tabIndex={-1}
              onClick={(event) => {
                if (
                  card.behavior !== "typed" &&
                  event.currentTarget.contains(event.target as Node) &&
                  !(event.target as HTMLElement).closest(
                    'a,button,input,audio,.audio-clip,[role="menuitem"]',
                  )
                )
                  reveal();
              }}
            >
              <div className="study-card-header">
                <span
                  className={`deck-badge color-${data.decks.find((d) => d.id === card.deckId)?.color ?? "violet"}`}
                >
                  {card.deckName}
                </span>
                <div className="inline">
                  <span className="card-side-label">
                    {revealed ? "ANSWER" : "QUESTION"}
                  </span>
                  <Menu
                    label="Study card options"
                    items={[
                      {
                        label: "Edit card",
                        icon: <Pencil size={15} />,
                        action: () =>
                          void navigate({ page: "editor", cardId: card.id }),
                      },
                      {
                        label: "Card information",
                        icon: <Info size={15} />,
                        action: () => setDetails(true),
                      },
                      {
                        label: "Bury until tomorrow",
                        icon: <Archive size={15} />,
                        action: () => void manage("bury"),
                        separator: true,
                      },
                      {
                        label: "Suspend card",
                        icon: <Pause size={15} />,
                        action: () => void manage("suspend"),
                      },
                    ]}
                  />
                </div>
              </div>
              <div className="study-card-body">
                {revealed && (
                  <p className="question-context">
                    {extractText(
                      (card.behavior === "reversed"
                        ? card.back
                        : card.front) as Doc,
                    )}
                  </p>
                )}
                <ContentRender
                  directory={data.mediaDir}
                  playbackKey={`${card.id}-${card.revision}-${revealed ? "answer" : "question"}`}
                  autoplay={data.preferences.audioAutoplay && !details && !busy}
                  value={
                    revealed
                      ? card.behavior === "reversed"
                        ? card.front
                        : card.back
                      : card.behavior === "reversed"
                        ? card.back
                        : card.front
                  }
                />
                {card.behavior === "typed" && !revealed && (
                  <form
                    className="typed-answer"
                    onSubmit={(e) => {
                      e.preventDefault();
                      reveal();
                    }}
                  >
                    <Keyboard size={18} />
                    <input
                      autoFocus
                      aria-label="Your answer"
                      placeholder="Type your answer…"
                      value={typed}
                      onChange={(e) => setTyped(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          reveal();
                        }
                      }}
                    />
                    <kbd>↵</kbd>
                  </form>
                )}
                {card.behavior === "typed" && revealed && (
                  <AnswerComparison
                    answer={typed}
                    expected={extractText(card.back as Doc)}
                  />
                )}
              </div>
              <div className="study-card-footer">
                {card.tags.slice(0, 4).map((t) => (
                  <span key={t} className="subtle-tag">
                    {t}
                  </span>
                ))}
                {!revealed && card.behavior !== "typed" && (
                  <span className="reveal-hint">
                    <Eye size={15} />
                    Click to reveal
                  </span>
                )}
                {revealed && (
                  <span className="reveal-hint">
                    How well did you remember?
                  </span>
                )}
              </div>
            </div>
          </div>
          <div className="study-controls">
            {!revealed ? (
              <>
                <Button
                  className="reveal-button"
                  variant="primary"
                  onClick={reveal}
                >
                  <Eye size={18} />
                  {card.behavior === "typed" ? "Check answer" : "Reveal answer"}
                  <kbd>{card.behavior === "typed" ? "Enter" : "Space"}</kbd>
                </Button>
                <p>Take a breath. Try to recall before you reveal.</p>
              </>
            ) : (
              <>
                <div className="grade-buttons">
                  {view.options.map((option, i) => {
                    const Icon = [RotateCcw, Minus, Check, ChevronsRight][i];
                    return (
                      <button
                        key={option.grade}
                        className={`grade-button grade-${option.grade}`}
                        disabled={busy}
                        onClick={() => void grade(option.grade)}
                      >
                        <span className="grade-interval">
                          {option.interval}
                        </span>
                        <span>
                          <Icon size={19} />
                          {option.label}
                        </span>
                        <kbd>{option.grade}</kbd>
                      </button>
                    );
                  })}
                </div>
                <p>
                  Grade your recall honestly. Tala will find the next moment to
                  review.
                </p>
              </>
            )}
          </div>
          {details && (
            <CardDetails cardId={card.id} onClose={() => setDetails(false)} />
          )}
        </>
      ) : view.finished ? (
        <div className="session-finished">
          <span className="completion-icon">
            <Check size={30} />
          </span>
          <h1>A little more, remembered.</h1>
          <p>
            {number(session.completed.length)} cards completed
            {session.skipped.length
              ? ` · ${session.skipped.length} skipped`
              : ""}
            . Your progress is saved.
          </p>
          <Button
            variant="primary"
            onClick={() => void navigate({ page: "today" })}
          >
            Back to Today
          </Button>
          {data.undoLabel && (
            <Button variant="ghost" onClick={undo}>
              <Undo2 size={16} />
              Undo last review
            </Button>
          )}
        </div>
      ) : (
        <div className="session-finished waiting">
          <span className="completion-icon">
            <Clock3 size={29} />
          </span>
          <h1>Let it settle for a moment.</h1>
          <p>
            {view.nextDue
              ? `Your next learning card is ready ${relativeDue(view.nextDue).toLowerCase()}.`
              : "Your remaining cards will be ready soon."}
            <br />
            You can leave, or stay here and we’ll continue automatically.
          </p>
          <Button onClick={() => void navigate({ page: "today" })}>
            Return to Today
          </Button>
        </div>
      )}
      <footer className="study-bottom">
        <span>
          <i className="status-dot" />
          Saved locally
        </span>
        <span>Session progress · repeated learning cards remain pending</span>
      </footer>
    </div>
  );
}
