import {
  Component,
  useCallback,
  useEffect,
  useRef,
  useState,
  lazy,
  Suspense,
  type ReactNode,
} from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import * as Tooltip from "@radix-ui/react-tooltip";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Sun,
  Layers,
  Plus,
  Library,
  ChartNoAxesCombined,
  Settings,
  Flame,
  HardDrive,
  X,
  CheckCircle2,
  AlertCircle,
  LoaderCircle,
} from "lucide-react";
import type { Deck } from "./bindings/Deck";
import { errorMessage, native, rpc } from "./lib/api";
import { isTyping } from "./lib/format";
import { TalaContext, type Route, type ConfirmOptions } from "./lib/context";
import { Button, ErrorPanel, Loading, Modal, Progress } from "./components/ui";
import DeckDialog from "./components/DeckDialog";
import { ImportDialog, ExportDialog } from "./components/TransferDialogs";
import TodayPage from "./pages/Today";
import DecksPage, { DeckOverview } from "./pages/Decks";
const EditorPage = lazy(() => import("./pages/Editor"));
const StudyPage = lazy(() => import("./pages/Study"));
const BrowsePage = lazy(() => import("./pages/Browse"));
const StatisticsPage = lazy(() => import("./pages/Statistics"));
const SettingsPage = lazy(() => import("./pages/Settings"));

type ConfirmState = ConfirmOptions & { resolve: (value: boolean) => void };
type AskState = {
  title: string;
  label: string;
  value: string;
  resolve: (value: string | null) => void;
};
/** Owns navigation and shared dialogs so all routes honor unsaved editor state. */
function Workspace() {
  const client = useQueryClient();
  const [route, setRoute] = useState<Route>({ page: "today" });
  const [toast, setToast] = useState<{
    message: string;
    error: boolean;
  } | null>(null);
  const [confirmState, setConfirm] = useState<ConfirmState | null>(null);
  const [askState, setAsk] = useState<AskState | null>(null);
  const [deckDialog, setDeckDialog] = useState<{
    deck?: Deck;
    parentId?: string;
  } | null>(null);
  const [transfer, setTransfer] = useState<{
    type: "import" | "export";
    deckId?: string;
    ids?: string[];
  } | null>(null);
  const [job, setJob] = useState<string | null>(null);
  const dirtyRef = useRef(false);
  const collection = useQuery({
    queryKey: ["bootstrap"],
    queryFn: () => rpc({ action: "bootstrap" }),
    refetchInterval: 30000,
  });
  const notify = useCallback(
    (message: string, error = false) => setToast({ message, error }),
    [],
  );
  const confirm = useCallback(
    (options: ConfirmOptions) =>
      new Promise<boolean>((resolve) => setConfirm({ ...options, resolve })),
    [],
  );
  const ask = useCallback(
    (title: string, label: string, value = "") =>
      new Promise<string | null>((resolve) =>
        setAsk({ title, label, value, resolve }),
      ),
    [],
  );
  const setDirty = useCallback(
    (dirty: boolean) => {
      dirtyRef.current = dirty;
      void rpc({ action: "editor_dirty", payload: { dirty } }).catch((error) =>
        notify(errorMessage(error), true),
      );
    },
    [notify],
  );
  const canLeave = useCallback(async () => {
    if (
      dirtyRef.current &&
      !(await confirm({
        title: "Leave without saving?",
        message:
          "Your unsaved card changes will be discarded. Saved notes and scheduling are not affected.",
        confirm: "Discard changes",
        danger: true,
      }))
    )
      return false;
    dirtyRef.current = false;
    await rpc({ action: "editor_dirty", payload: { dirty: false } });
    return true;
  }, [confirm]);
  const navigate = useCallback(
    async (next: Route) => {
      if (await canLeave()) setRoute(next);
    },
    [canLeave],
  );
  useEffect(() => {
    document.querySelector("main")?.scrollTo({ top: 0, left: 0 });
    window.scrollTo(0, 0);
  }, [route]);
  const run = useCallback(
    async <T,>(
      operation: () => Promise<T>,
      success?: string,
    ): Promise<T | undefined> => {
      try {
        const result = await operation();
        await client.invalidateQueries();
        if (success) notify(success);
        return result;
      } catch (error) {
        notify(errorMessage(error), true);
        return undefined;
      }
    },
    [client, notify],
  );
  const startStudy = useCallback(
    async (deckId?: string) => {
      if (!(await canLeave())) return;
      const result = await run(() =>
        rpc({ action: "start_study", payload: { deckId: deckId ?? null } }),
      );
      if (result) {
        client.setQueryData(["study"], result);
        setRoute({ page: "study" });
      }
    },
    [canLeave, run, client],
  );
  useEffect(() => {
    if (!toast || toast.error) return;
    const timer = setTimeout(() => setToast(null), 5500);
    return () => clearTimeout(timer);
  }, [toast]);
  useEffect(() => {
    // Listener registration is async; clean up even if it finishes after unmount.
    let disposed = false;
    let offJob: (() => void) | undefined;
    let offClose: (() => void) | undefined;
    let offQuit: (() => void) | undefined;
    let quitting = false;
    void listen("tala:quit-requested", async () => {
      if (quitting) return;
      quitting = true;
      try {
        if (await canLeave()) await rpc({ action: "quit_app" });
      } finally {
        quitting = false;
      }
    }).then((off) => {
      if (disposed) off();
      else offQuit = off;
    });
    void listen<{ label: string; running: boolean }>("tala:job", (event) =>
      setJob(event.payload.running ? event.payload.label : null),
    ).then((off) => {
      if (disposed) off();
      else offJob = off;
    });
    void getCurrentWindow()
      .onCloseRequested(async (event) => {
        if (dirtyRef.current) {
          event.preventDefault();
          if (await canLeave()) await getCurrentWindow().destroy();
        }
      })
      .then((off) => {
        if (disposed) off();
        else offClose = off;
      });
    return () => {
      disposed = true;
      offJob?.();
      offClose?.();
      offQuit?.();
    };
  }, [canLeave]);
  useEffect(() => {
    document.documentElement.style.setProperty(
      "--ui-scale",
      String((collection.data?.preferences.scale ?? 100) / 100),
    );
  }, [collection.data?.preferences.scale]);
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (
        !(event.metaKey || event.ctrlKey) ||
        document.querySelector('[role="dialog"]')
      )
        return;
      const key = event.key.toLowerCase();
      if (key === "n") {
        event.preventDefault();
        void navigate({ page: "editor" });
      }
      if (key === "b" && !isTyping(event.target)) {
        event.preventDefault();
        void navigate({ page: "browse" });
      }
      if (key === "f") {
        const field = document.querySelector<HTMLInputElement>(
          'input[aria-label^="Search"]',
        );
        if (field) {
          event.preventDefault();
          field.focus();
          field.select();
        }
      }
      if (
        key === "z" &&
        route.page !== "study" &&
        !isTyping(event.target) &&
        collection.data?.undoLabel
      ) {
        event.preventDefault();
        void run(() => rpc({ action: "undo" }), "Action undone");
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [navigate, route.page, collection.data?.undoLabel, run]);
  if (collection.isPending)
    return (
      <div className="startup">
        <img src="/tala-mark.png" alt="Tala" />
        <Loading />
      </div>
    );
  if (collection.error)
    return (
      <div className="startup">
        <img src="/tala-mark.png" alt="Tala" />
        <h1>Your collection needs attention</h1>
        <ErrorPanel
          message={errorMessage(collection.error)}
          retry={() => void collection.refetch()}
        />
        <Button onClick={() => void rpc({ action: "open_data_directory" })}>
          Open data and backups folder
        </Button>
        <Button
          variant="primary"
          busy={!!job}
          onClick={() => void run(() => rpc({ action: "recover_collection" }))}
        >
          Restore a Tala backup
        </Button>
        {toast?.error && <p role="alert">{toast.message}</p>}
        <p>Tala has not replaced or reset your collection.</p>
      </div>
    );
  const data = collection.data;
  const due = data.today.newCount + data.today.learning + data.today.review;
  function importCards(deckId?: string) {
    setTransfer({ type: "import", deckId });
  }
  function content() {
    switch (route.page) {
      case "today":
        return <TodayPage />;
      case "decks":
        return <DecksPage />;
      case "deck":
        return <DeckOverview id={route.id} />;
      case "editor":
        return (
          <EditorPage
            key={route.cardId ?? route.deckId ?? "new"}
            cardId={route.cardId}
            deckId={route.deckId}
          />
        );
      case "study":
        return <StudyPage />;
      case "browse":
        return (
          <BrowsePage
            key={`${route.deckId}-${route.trash}-${route.tag}`}
            deckId={route.deckId}
            trash={route.trash}
            tag={route.tag}
          />
        );
      case "statistics":
        return <StatisticsPage />;
      case "settings":
        return <SettingsPage />;
    }
  }
  const nav = [
    { page: "today", label: "Today", icon: Sun },
    { page: "decks", label: "Decks", icon: Layers },
    { page: "editor", label: "Add card", icon: Plus },
    { page: "browse", label: "Browse", icon: Library },
    { page: "statistics", label: "Statistics", icon: ChartNoAxesCombined },
    { page: "settings", label: "Settings", icon: Settings },
  ] as const;
  return (
    <TalaContext.Provider
      value={{
        data,
        route,
        navigate,
        run,
        confirm,
        ask,
        notify,
        editDeck: (deck, parentId) => setDeckDialog({ deck, parentId }),
        startStudy,
        setDirty,
        importCards,
        exportCards: (deckId, ids) =>
          setTransfer({ type: "export", deckId, ids }),
      }}
    >
      <Tooltip.Provider delayDuration={350}>
        <div
          className={`app-shell ${route.page === "study" ? "in-study" : ""}`}
        >
          <div className="window-drag" data-tauri-drag-region />
          <aside className="sidebar">
            <button
              className="brand"
              onClick={() => void navigate({ page: "today" })}
              aria-label="Tala Today"
            >
              <img src="/tala-mark.png" alt="" />
              <span>
                tala<span className="brand-period">.</span>
              </span>
            </button>
            <nav aria-label="Main navigation">
              {nav.map((item) => {
                const active =
                  route.page === item.page ||
                  (item.page === "today" && route.page === "study") ||
                  (item.page === "decks" && route.page === "deck");
                return (
                  <button
                    key={item.page}
                    className={`nav-item ${active ? "active" : ""}`}
                    aria-current={active ? "page" : undefined}
                    onClick={() => void navigate({ page: item.page })}
                    title={item.label}
                  >
                    <item.icon size={18} />
                    <span>{item.label}</span>
                    {item.page === "today" && due > 0 && (
                      <small className="nav-count">{due}</small>
                    )}
                    {item.page === "editor" && <kbd>⌘ N</kbd>}
                  </button>
                );
              })}
            </nav>
            {data.decks.length > 0 && (
              <section className="sidebar-decks">
                <h2>YOUR DECKS</h2>
                {data.decks.slice(0, 5).map((deck) => (
                  <button
                    key={deck.id}
                    className="sidebar-deck"
                    onClick={() => void navigate({ page: "deck", id: deck.id })}
                  >
                    <i className={`color-dot tiny color-${deck.color}`} />
                    <span>{deck.name}</span>
                  </button>
                ))}
                <button
                  className="sidebar-deck new"
                  onClick={() => setDeckDialog({})}
                >
                  <Plus size={15} />
                  <span>New deck</span>
                </button>
              </section>
            )}
            <div className="sidebar-bottom">
              <div className="streak-card">
                <div>
                  <Flame size={19} />
                  <span>Keep the spark</span>
                </div>
                <strong>
                  {data.today.streak}
                  <span> day streak</span>
                </strong>
                <Progress
                  value={Math.min(data.today.streak / 7, 1)}
                  label="Progress toward a seven-day study streak"
                />
                <small>
                  {data.today.streak
                    ? "A little practice goes a long way."
                    : "Your first review starts it."}
                </small>
              </div>
              <div className="local-status">
                <HardDrive size={14} />
                <span>Local. Private. Yours.</span>
                <i className="status-dot" />
              </div>
            </div>
          </aside>
          <main
            className="main-content"
            key={route.page === "study" ? "study" : "main"}
          >
            {data.backupWarning && route.page !== "settings" && (
              <button
                className="backup-warning"
                onClick={() => void navigate({ page: "settings" })}
              >
                <AlertCircle size={15} />A backup needs attention. Open Settings
                to review.
              </button>
            )}
            <Suspense fallback={<Loading />}>{content()}</Suspense>
          </main>
          {job && (
            <div className="job-status" role="status">
              <LoaderCircle className="spin" size={16} />
              {job}…
            </div>
          )}
          {toast && (
            <div
              className={`toast ${toast.error ? "error" : ""}`}
              role={toast.error ? "alert" : "status"}
            >
              {toast.error ? (
                <AlertCircle size={19} />
              ) : (
                <CheckCircle2 size={19} />
              )}
              <span>{toast.message}</span>
              <button
                className="icon-button"
                aria-label="Dismiss notification"
                onClick={() => setToast(null)}
              >
                <X size={16} />
              </button>
            </div>
          )}
        </div>
        {deckDialog && (
          <DeckDialog
            deck={deckDialog.deck}
            parentId={deckDialog.parentId}
            onClose={() => setDeckDialog(null)}
          />
        )}{" "}
        {transfer?.type === "import" && (
          <ImportDialog
            deckId={transfer.deckId}
            onClose={() => setTransfer(null)}
          />
        )}{" "}
        {transfer?.type === "export" && (
          <ExportDialog
            deckId={transfer.deckId}
            ids={transfer.ids}
            onClose={() => setTransfer(null)}
          />
        )}{" "}
        {confirmState && (
          <Modal
            title={confirmState.title}
            description={confirmState.message}
            open
            onClose={() => {
              confirmState.resolve(false);
              setConfirm(null);
            }}
          >
            <div className="modal-actions">
              <Button
                onClick={() => {
                  confirmState.resolve(false);
                  setConfirm(null);
                }}
              >
                Cancel
              </Button>
              <Button
                variant={confirmState.danger ? "danger" : "primary"}
                onClick={() => {
                  confirmState.resolve(true);
                  setConfirm(null);
                }}
              >
                {confirmState.confirm ?? "Confirm"}
              </Button>
            </div>
          </Modal>
        )}
        {askState && (
          <Modal
            title={askState.title}
            open
            onClose={() => {
              askState.resolve(null);
              setAsk(null);
            }}
          >
            <form
              onSubmit={(event) => {
                event.preventDefault();
                askState.resolve(askState.value);
                setAsk(null);
              }}
            >
              <label className="field">
                <span>{askState.label}</span>
                <input
                  autoFocus
                  value={askState.value}
                  onChange={(event) =>
                    setAsk({ ...askState, value: event.target.value })
                  }
                />
              </label>
              <div className="modal-actions">
                <Button
                  onClick={() => {
                    askState.resolve(null);
                    setAsk(null);
                  }}
                >
                  Cancel
                </Button>
                <Button variant="primary" type="submit">
                  Apply
                </Button>
              </div>
            </form>
          </Modal>
        )}
      </Tooltip.Provider>
    </TalaContext.Provider>
  );
}
class ErrorBoundary extends Component<
  { children: ReactNode },
  { error: boolean }
> {
  state = { error: false };
  static getDerivedStateFromError() {
    return { error: true };
  }
  render() {
    return this.state.error ? (
      <div className="startup">
        <h1>Let’s reopen your workspace</h1>
        <p>
          The interface encountered a problem. Your saved collection is
          unchanged.
        </p>
        <Button onClick={() => window.location.reload()}>Reload Tala</Button>
      </div>
    ) : (
      this.props.children
    );
  }
}
export default function App() {
  if (!native())
    return (
      <div className="startup">
        <img src="/tala-mark.png" alt="Tala" />
        <h1>Your space to remember.</h1>
        <p>
          Tala is a native desktop app. Start it with{" "}
          <code>pnpm tauri dev</code> to use your local collection.
        </p>
      </div>
    );
  return (
    <ErrorBoundary>
      <Workspace />
    </ErrorBoundary>
  );
}
