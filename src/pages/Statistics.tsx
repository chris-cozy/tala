import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  PieChart,
  Pie,
  Cell,
  BarChart,
  Bar,
} from "recharts";
import {
  BarChart3,
  Clock3,
  Flame,
  Target,
  TrendingUp,
  BookOpen,
} from "lucide-react";
import { rpc, errorMessage } from "../lib/api";
import { number, percent, duration } from "../lib/format";
import { useTala } from "../lib/context";
import { ErrorPanel, Loading, PageHeader, Progress } from "../components/ui";
const colors = ["#9476ed", "#eba858", "#51bfa4", "#e57683"];
const shortDay = (s: string) =>
  new Date(`${s}T12:00:00`).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
export default function StatisticsPage() {
  const { navigate } = useTala();
  const [range, setRange] = useState("30");
  const query = useQuery({
    queryKey: ["statistics", range],
    queryFn: () => rpc({ action: "statistics", payload: { range } }),
  });
  if (query.isPending) return <Loading label="Gathering your study history…" />;
  if (query.error)
    return (
      <ErrorPanel
        message={errorMessage(query.error)}
        retry={() => void query.refetch()}
      />
    );
  const s = query.data;
  const stages = s.states.slice(0, 4).filter((v) => v.count);
  const total = stages.reduce((n, v) => n + v.count, 0);
  const metrics = [
    {
      title: "Cards reviewed",
      value: number(s.summary.reviewed),
      icon: BookOpen,
      sub: "Completed reviews in this period",
    },
    {
      title: "Study time",
      value: duration(s.summary.seconds),
      icon: Clock3,
      sub: s.summary.reviewed
        ? `${(s.summary.seconds / s.summary.reviewed).toFixed(1)} sec average answer`
        : "Active reviewing time",
    },
    {
      title: "Recall rate",
      value: s.summary.reviewed ? percent(s.summary.recall) : "—",
      icon: Target,
      sub: "Hard, Good, or Easy responses",
    },
    {
      title: "Study streak",
      value: `${s.currentStreak} ${s.currentStreak === 1 ? "day" : "days"}`,
      icon: Flame,
      sub: `Longest streak · ${s.longestStreak} ${s.longestStreak === 1 ? "day" : "days"}`,
    },
  ];
  return (
    <div className="page statistics-page">
      <PageHeader
        eyebrow="SEE WHAT’S TAKING ROOT"
        title="Your practice, in perspective."
        subtitle="Small efforts add up. Here’s how yours are growing."
        actions={
          <div className="segmented-control" aria-label="Statistics time range">
            {[
              ["7", "7d"],
              ["30", "30d"],
              ["90", "90d"],
              ["365", "1y"],
              ["all", "All time"],
            ].map(([value, label]) => (
              <button
                key={value}
                className={range === value ? "selected" : ""}
                onClick={() => setRange(value)}
                aria-pressed={range === value}
              >
                {label}
              </button>
            ))}
          </div>
        }
      />
      <div className="metrics-row">
        {metrics.map((m, i) => (
          <div className="metric-card stat-metric" key={m.title}>
            <div className="metric-label">
              <span>{m.title}</span>
              <m.icon
                size={17}
                className={`text-${["violet", "blue", "teal", "orange"][i]}`}
              />
            </div>
            <strong>{m.value}</strong>
            <small>{m.sub}</small>
          </div>
        ))}
      </div>
      {!s.summary.reviewed && (
        <div className="notice">
          <BarChart3 size={18} />
          Your history starts with your first review. Current card stages and
          upcoming due dates are shown below.
        </div>
      )}
      <div className="stats-main-grid">
        <section className="panel review-chart">
          <div className="panel-heading">
            <div>
              <h2>Reviews over time</h2>
              <p>
                {number(s.summary.reviewed)} reviews ·{" "}
                {number(s.summary.learned)} cards first learned
              </p>
            </div>
            {s.previous.reviewed > 0 && (
              <span className="stat-change">
                <TrendingUp size={14} />
                {Math.round(
                  ((s.summary.reviewed - s.previous.reviewed) /
                    s.previous.reviewed) *
                    100,
                )}
                % vs previous period
              </span>
            )}
          </div>
          <div className="chart-large">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart
                data={s.days}
                margin={{ top: 10, right: 14, left: -18, bottom: 0 }}
              >
                <defs>
                  <linearGradient id="review-fill" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="#9975ff" stopOpacity={0.32} />
                    <stop offset="100%" stopColor="#9975ff" stopOpacity={0} />
                  </linearGradient>
                </defs>
                <CartesianGrid vertical={false} stroke="#ffffff08" />
                <XAxis
                  dataKey="day"
                  tickFormatter={shortDay}
                  tick={{ fill: "#788398", fontSize: 11 }}
                  axisLine={false}
                  tickLine={false}
                  minTickGap={50}
                />
                <YAxis
                  allowDecimals={false}
                  tick={{ fill: "#788398", fontSize: 11 }}
                  axisLine={false}
                  tickLine={false}
                />
                <Tooltip
                  contentStyle={{
                    background: "#171d2a",
                    border: "1px solid #ffffff15",
                    borderRadius: 10,
                    color: "#eee",
                  }}
                  labelFormatter={(v) => shortDay(String(v))}
                />
                <Area
                  type="monotone"
                  dataKey="count"
                  name="Reviews"
                  stroke="#a286f7"
                  strokeWidth={2.2}
                  fill="url(#review-fill)"
                  isAnimationActive={false}
                />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </section>
        <section className="panel stages-panel">
          <div className="panel-heading">
            <div>
              <h2>Cards by learning stage</h2>
              <p>Current collection snapshot</p>
            </div>
          </div>
          {total ? (
            <div className="donut-layout">
              <div className="donut">
                <ResponsiveContainer width="100%" height="100%">
                  <PieChart>
                    <Pie
                      data={stages}
                      dataKey="count"
                      nameKey="name"
                      innerRadius="72%"
                      outerRadius="95%"
                      startAngle={90}
                      endAngle={-270}
                      paddingAngle={3}
                      stroke="none"
                      isAnimationActive={false}
                    >
                      {stages.map((item) => (
                        <Cell
                          key={item.name}
                          fill={
                            colors[
                              [
                                "New",
                                "Learning",
                                "Review",
                                "Relearning",
                              ].indexOf(item.name)
                            ]
                          }
                        />
                      ))}
                    </Pie>
                  </PieChart>
                </ResponsiveContainer>
                <div className="donut-center">
                  <strong>{number(total)}</strong>
                  <small>active cards</small>
                </div>
              </div>
              <div className="chart-legend">
                {s.states.slice(0, 4).map((v, i) => (
                  <div key={v.name}>
                    <i style={{ background: colors[i] }} />
                    <span>{v.name}</span>
                    <strong>{number(v.count)}</strong>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="chart-empty">Your cards will appear here.</div>
          )}
          <p className="panel-footnote">
            {s.states[4].count} suspended · {s.states[5].count} buried
          </p>
        </section>
      </div>
      <div className="stats-secondary-grid">
        <section className="panel">
          <div className="panel-heading">
            <div>
              <h2>How recall feels</h2>
              <p>Grades in the selected period</p>
            </div>
          </div>
          <div className="grade-distribution">
            {s.grades.map((g, i) => (
              <div key={g.name}>
                <span>
                  <i
                    style={{
                      background: ["#e57683", "#eba858", "#51bfa4", "#70a8df"][
                        i
                      ],
                    }}
                  />
                  {g.name}
                </span>
                <div className="distribution-track">
                  <div
                    style={{
                      width: `${s.summary.reviewed ? (g.count / s.summary.reviewed) * 100 : 0}%`,
                      background: ["#e57683", "#eba858", "#51bfa4", "#70a8df"][
                        i
                      ],
                    }}
                  />
                </div>
                <strong>
                  {g.count ? percent(g.count / s.summary.reviewed) : "0%"}
                </strong>
              </div>
            ))}
          </div>
        </section>
        <section className="panel">
          <div className="panel-heading">
            <div>
              <h2>On the horizon</h2>
              <p>Next 14 days · stored due dates, before future study</p>
            </div>
          </div>
          <div className="forecast-chart">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={s.forecast}>
                <XAxis
                  dataKey="day"
                  tickFormatter={shortDay}
                  tick={{ fill: "#788398", fontSize: 10 }}
                  axisLine={false}
                  tickLine={false}
                  minTickGap={35}
                />
                <Tooltip
                  contentStyle={{
                    background: "#171d2a",
                    border: "1px solid #ffffff15",
                    borderRadius: 10,
                  }}
                  labelFormatter={(v) => shortDay(String(v))}
                />
                <Bar
                  dataKey="count"
                  name="Due"
                  fill="#7360b5"
                  radius={[4, 4, 0, 0]}
                  isAnimationActive={false}
                />
              </BarChart>
            </ResponsiveContainer>
          </div>
        </section>
      </div>
      <section className="panel deck-progress-panel">
        <div className="panel-heading">
          <div>
            <h2>Across your decks</h2>
            <p>Cards in the long-term Review stage, not a measure of mastery</p>
          </div>
        </div>
        {s.decks.length ? (
          s.decks.map((deck) => (
            <button
              key={deck.id}
              className="deck-progress-row"
              onClick={() => void navigate({ page: "deck", id: deck.id })}
            >
              <span>
                <i className={`color-dot tiny color-${deck.color}`} />
                {deck.name}
              </span>
              <Progress
                value={deck.total ? deck.inReview / deck.total : 0}
                label={`${deck.name} cards in Review`}
              />
              <small>
                {number(deck.inReview)} / {number(deck.total)} in Review
              </small>
              <small>{number(deck.due)} ready</small>
            </button>
          ))
        ) : (
          <p className="panel-footnote">
            Create a deck to begin tracking progress.
          </p>
        )}
      </section>
      <details className="panel scheduler-insights">
        <summary>
          Behind the timing <span>FSRS insights</span>
        </summary>
        <p>
          Estimated retention is the average predicted recall across your
          unsuspended Review cards. It is a model estimate, separate from your
          recorded recall rate.
        </p>
        <div className="metrics-row">
          <div>
            <small>Estimated retention</small>
            <strong>
              {s.estimatedRetention == null
                ? "—"
                : percent(s.estimatedRetention)}
            </strong>
          </div>
          <div>
            <small>Average stability</small>
            <strong>{s.averageStability?.toFixed(1) ?? "—"} days</strong>
          </div>
          <div>
            <small>Average difficulty</small>
            <strong>{s.averageDifficulty?.toFixed(1) ?? "—"} / 10</strong>
          </div>
          <div>
            <small>New cards learned</small>
            <strong>{number(s.summary.learned)}</strong>
          </div>
        </div>
      </details>
    </div>
  );
}
