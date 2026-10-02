import {
  ArrowRight,
  BookOpen,
  Check,
  Clock3,
  Flame,
  Layers,
  Play,
  Plus,
} from "lucide-react";
import { useTala } from "../lib/context";
import { number, relativeDue } from "../lib/format";
import { Button, PageHeader, Progress } from "../components/ui";
import { DeckTile } from "./Decks";
export default function TodayPage() {
  const { data, editDeck, navigate, startStudy } = useTala();
  const today = data.today;
  const due = today.newCount + today.learning + today.review;
  const hasCards = data.decks.some((d) => d.total);
  const limited = data.decks.reduce(
    (n, d) => n + d.limitedNew + d.limitedReview,
    0,
  );
  const recent = data.decks
    .filter((deck) => !deck.parentId)
    .sort(
      (a, b) =>
        b.subtreeCounts.due - a.subtreeCounts.due || b.updatedAt - a.updatedAt,
    )
    .slice(0, 4);
  return (
    <div className="page today-page">
      <PageHeader
        eyebrow={new Date()
          .toLocaleDateString(undefined, {
            weekday: "long",
            month: "long",
            day: "numeric",
          })
          .toUpperCase()}
        title="A little today. A lot remembered."
        subtitle="Build understanding, one thoughtful repetition at a time."
        actions={
          <Button onClick={() => void navigate({ page: "editor" })}>
            <Plus size={17} />
            Add card
          </Button>
        }
      />
      <section className={`today-hero ${!hasCards ? "first-launch" : ""}`}>
        <div className="hero-ambient" />
        <img className="hero-mark" src="/tala-mark.png" alt="" />
        <div className="today-hero-content">
          <span className="eyebrow">
            <span className="status-dot" />
            YOUR DAILY PRACTICE
          </span>
          <h2>
            {!hasCards
              ? "Start with a spark."
              : due
                ? "Make it stay with you."
                : today.nextDue
                  ? "A moment to let it settle."
                  : "Room to breathe."}
          </h2>
          <p>
            {!hasCards
              ? "Gather the things you want to know. Tala will help you come back to them, just when it matters."
              : due
                ? `${number(due)} cards are ready for your attention. Pick up where you left off.`
                : today.nextDue
                  ? `Your next cards are available ${relativeDue(today.nextDue).toLowerCase()}. Your progress is saved.`
                  : limited
                    ? "You’ve reached your daily limits. Come back tomorrow, or adjust a deck’s limits."
                    : "You’re all caught up. What you learned today is on its way to becoming something you know."}
          </p>
          {!hasCards ? (
            <Button
              variant="primary"
              onClick={() =>
                data.decks.length
                  ? void navigate({ page: "editor" })
                  : editDeck()
              }
            >
              <Plus size={17} />
              {data.decks.length
                ? "Add your first card"
                : "Create your first deck"}
              <ArrowRight size={17} />
            </Button>
          ) : due ? (
            <Button variant="primary" onClick={() => void startStudy()}>
              <Play size={17} fill="currentColor" />
              Start studying
              <ArrowRight size={17} />
            </Button>
          ) : data.session && today.nextDue ? (
            <Button onClick={() => void navigate({ page: "study" })}>
              <Clock3 size={17} />
              Return to session
            </Button>
          ) : (
            <span className="caught-up">
              <Check size={18} />
              Scheduled study complete for now
            </span>
          )}
        </div>
        <div className="hero-side-note">
          <span>Less cramming.</span>
          <span>More remembering.</span>
        </div>
      </section>
      <div className="metrics-row today-metrics">
        {[
          {
            name: "New to explore",
            value: today.newCount,
            icon: Layers,
            color: "violet",
          },
          {
            name: "Still taking root",
            value: today.learning,
            icon: BookOpen,
            color: "amber",
          },
          {
            name: "Ready to revisit",
            value: today.review,
            icon: Clock3,
            color: "teal",
          },
          {
            name: "Day study streak",
            value: today.streak,
            icon: Flame,
            color: "orange",
          },
        ].map((metric) => (
          <div className="metric-card" key={metric.name}>
            <div className="metric-label">
              <span>{metric.name}</span>
              <metric.icon size={17} className={`text-${metric.color}`} />
            </div>
            <strong>{number(metric.value)}</strong>
          </div>
        ))}
      </div>
      {hasCards && (
        <section className="daily-progress">
          <div className="inline">
            <span className="progress-check">
              <Check size={15} />
            </span>
            <span>
              <strong>{number(today.studied)} cards studied today</strong>
              <small>
                {due
                  ? `${number(due)} ready now, including repetitions`
                  : "Every review is a little investment in tomorrow."}
              </small>
            </span>
          </div>
          <div>
            <Progress
              value={today.studied / (today.studied + due || 1)}
              label="Cards studied today compared with cards currently ready"
            />
          </div>
        </section>
      )}
      <div className="section-heading">
        <div>
          <h2>
            {data.decks.length
              ? "Your next chapter"
              : "A collection that grows with you"}
          </h2>
          <p>
            {data.decks.length
              ? "Pick a deck and settle into your practice."
              : "No accounts. No distractions. Just you and what you want to learn."}
          </p>
        </div>
        {data.decks.length > 0 && (
          <Button
            variant="ghost"
            onClick={() => void navigate({ page: "decks" })}
          >
            All decks
            <ArrowRight size={16} />
          </Button>
        )}
      </div>
      {data.decks.length ? (
        <div className="deck-grid compact">
          {recent.map((d) => (
            <DeckTile key={d.id} deck={d} />
          ))}
        </div>
      ) : (
        <div className="onboarding-steps">
          {[
            {
              icon: Layers,
              title: "Make a home for an idea",
              text: "Organize what you’re learning into simple decks.",
            },
            {
              icon: BookOpen,
              title: "Put it in your own words",
              text: "Create cards with text, images, audio, and equations.",
            },
            {
              icon: Clock3,
              title: "Come back at the right time",
              text: "FSRS adapts each next review to your memory.",
            },
          ].map((item, i) => (
            <div key={item.title}>
              <span className="step-number">0{i + 1}</span>
              <item.icon size={21} />
              <h3>{item.title}</h3>
              <p>{item.text}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
