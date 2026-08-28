import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Pencil } from "lucide-react";
import { rpc, errorMessage } from "../lib/api";
import { useTala } from "../lib/context";
import { behaviorLabel, cardState, dateTime, duration } from "../lib/format";
import { Button, ErrorPanel, Loading, Modal } from "./ui";
import {
  ContentRender,
  AnswerComparison,
  extractText,
  type Doc,
} from "./RichContent";
export function CardDetails({
  cardId,
  onClose,
}: {
  cardId: string;
  onClose: () => void;
}) {
  const { data, navigate } = useTala();
  const [tab, setTab] = useState("preview");
  const [revealed, setRevealed] = useState(false);
  const [answer, setAnswer] = useState("");
  const [historyPage, setHistoryPage] = useState(0);
  const query = useQuery({
    queryKey: ["card", cardId],
    queryFn: () => rpc({ action: "get_card", payload: { cardId } }),
  });
  const history = useQuery({
    queryKey: ["history", cardId, historyPage],
    queryFn: () =>
      rpc({
        action: "history",
        payload: { cardId, offset: historyPage * 100 },
      }),
    enabled: tab === "history",
  });
  const card = query.data;
  return (
    <Modal
      title="A closer look"
      description={
        card ? `${card.deckName} · ${behaviorLabel(card.behavior)}` : undefined
      }
      open
      onClose={onClose}
      wide
    >
      {query.isPending ? (
        <Loading />
      ) : query.error ? (
        <ErrorPanel message={errorMessage(query.error)} />
      ) : (
        card && (
          <>
            <div className="detail-tabs">
              <div className="segmented-control">
                {["preview", "information", "history"].map((value) => (
                  <button
                    key={value}
                    className={tab === value ? "selected" : ""}
                    onClick={() => setTab(value)}
                  >
                    {value}
                  </button>
                ))}
              </div>
              <Button
                disabled={card.deletedAt != null}
                onClick={() => {
                  onClose();
                  void navigate({ page: "editor", cardId });
                }}
              >
                <Pencil size={15} />
                Edit card
              </Button>
            </div>
            {tab === "preview" ? (
              <>
                <div className="preview-card">
                  <span className="eyebrow">
                    {revealed ? "ANSWER" : "QUESTION"}
                  </span>
                  <ContentRender
                    directory={data.mediaDir}
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
                    <input
                      aria-label="Preview answer"
                      value={answer}
                      placeholder="Type your answer…"
                      onChange={(e) => setAnswer(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") setRevealed(true);
                      }}
                    />
                  )}{" "}
                  {card.behavior === "typed" && revealed && (
                    <AnswerComparison
                      answer={answer}
                      expected={extractText(card.back as Doc)}
                    />
                  )}
                </div>
                <div className="modal-actions">
                  <Button onClick={() => setRevealed(!revealed)}>
                    {revealed ? "Show question" : "Reveal answer"}
                  </Button>
                </div>
              </>
            ) : tab === "information" ? (
              <>
                <dl className="card-information">
                  {[
                    ["State", cardState(card)],
                    ["Due", dateTime(card.schedule.due)],
                    [
                      "Current interval",
                      card.schedule.phase === "review"
                        ? `${card.schedule.scheduledDays} days`
                        : card.schedule.lastReview
                          ? duration(
                              Math.max(
                                0,
                                card.schedule.due - card.schedule.lastReview,
                              ),
                            )
                          : "New",
                    ],
                    ["Reviews", String(card.schedule.reviewCount)],
                    ["Lapses", String(card.schedule.lapses)],
                    [
                      "Leech",
                      card.leech ? "Yes — consider rewriting this card" : "No",
                    ],
                    ["Created", dateTime(card.createdAt)],
                    ["Modified", dateTime(card.modifiedAt)],
                    ["Tags", card.tags.join(", ") || "None"],
                    ["Card ID", card.id],
                    ["Note ID", card.noteId],
                  ].map(([name, value]) => (
                    <div key={name}>
                      <dt>{name}</dt>
                      <dd>{value}</dd>
                    </div>
                  ))}
                </dl>
                <details className="scheduler-detail">
                  <summary>FSRS scheduling information</summary>
                  <dl className="card-information">
                    <div>
                      <dt>Stability</dt>
                      <dd>
                        {card.schedule.stability?.toFixed(2) ??
                          "Not established"}{" "}
                        days
                      </dd>
                    </div>
                    <div>
                      <dt>Difficulty</dt>
                      <dd>
                        {card.schedule.difficulty?.toFixed(2) ??
                          "Not established"}
                      </dd>
                    </div>
                    <div>
                      <dt>Desired retention</dt>
                      <dd>
                        {Math.round(
                          (data.decks.find((d) => d.id === card.deckId)
                            ?.settings.retention ?? 0.9) * 100,
                        )}
                        %
                      </dd>
                    </div>
                    <div>
                      <dt>Last reviewed</dt>
                      <dd>{dateTime(card.schedule.lastReview)}</dd>
                    </div>
                  </dl>
                </details>
              </>
            ) : (
              <div className="history-view">
                {history.isPending ? (
                  <Loading />
                ) : history.error ? (
                  <ErrorPanel message={errorMessage(history.error)} />
                ) : !history.data?.length ? (
                  <p className="muted">
                    No reviews on this page. Your first review will appear here.
                  </p>
                ) : (
                  <div className="history-table">
                    <table>
                      <thead>
                        <tr>
                          <th>Reviewed</th>
                          <th>Grade</th>
                          <th>Transition</th>
                          <th>Interval</th>
                          <th>Time</th>
                        </tr>
                      </thead>
                      <tbody>
                        {history.data.map((event) => (
                          <tr
                            key={event.id}
                            className={event.undoneAt ? "undone-review" : ""}
                          >
                            <td>
                              {dateTime(event.timestamp)}
                              {event.undoneAt && (
                                <small>Undone · retained in history</small>
                              )}
                            </td>
                            <td className={`text-grade-${event.grade}`}>
                              {
                                ["Again", "Hard", "Good", "Easy"][
                                  event.grade - 1
                                ]
                              }
                            </td>
                            <td>
                              {event.before.phase} → {event.after.phase}
                            </td>
                            <td>
                              {event.before.scheduledDays}d →{" "}
                              {event.after.phase === "review"
                                ? `${event.after.scheduledDays}d`
                                : duration(event.after.due - event.timestamp)}
                            </td>
                            <td>{duration(event.durationMs / 1000)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
                <div className="modal-actions">
                  <Button
                    disabled={!historyPage}
                    onClick={() => setHistoryPage((n) => n - 1)}
                  >
                    Previous
                  </Button>
                  <span className="muted small">Page {historyPage + 1}</span>
                  <Button
                    disabled={(history.data?.length ?? 0) < 100}
                    onClick={() => setHistoryPage((n) => n + 1)}
                  >
                    Next
                  </Button>
                </div>
              </div>
            )}
          </>
        )
      )}
    </Modal>
  );
}
