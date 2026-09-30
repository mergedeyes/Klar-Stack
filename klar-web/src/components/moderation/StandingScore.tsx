import type { MyStanding } from "@/lib/api";
import { MEASURE_LABELS } from "@/lib/moderation";

// The account's score as a bar from 0 to max_score, with a tick at each
// threshold (warning, suspensions) so it's clear how far the next one is.
export function StandingScore({ standing }: { standing: Pick<MyStanding, "score" | "max_score" | "thresholds"> }) {
  const pct = (n: number) => `${Math.min(100, (n / standing.max_score) * 100)}%`;
  const next = standing.thresholds.find((t) => t.score > standing.score);
  const color =
    standing.score >= 50 ? "bg-destructive" : standing.score >= 25 ? "bg-amber-500" : "bg-emerald-500";

  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between text-sm">
        <span>
          <strong className="text-lg">{standing.score}</strong>
          <span className="text-muted-foreground"> / {standing.max_score} points</span>
        </span>
        {next && (
          <span className="text-xs text-muted-foreground">
            {MEASURE_LABELS[next.measure]} at {next.score}
          </span>
        )}
      </div>
      <div
        className="relative h-2 rounded-full bg-muted"
        role="meter"
        aria-valuemin={0}
        aria-valuemax={standing.max_score}
        aria-valuenow={standing.score}
        aria-label="Account standing score"
      >
        <div className={`h-2 rounded-full ${color}`} style={{ width: pct(standing.score) }} />
        {standing.thresholds
          .filter((t) => t.score < standing.max_score)
          .map((t) => (
            <div
              key={t.score}
              className="absolute top-[-2px] h-3 w-px bg-foreground/40"
              style={{ left: pct(t.score) }}
              title={`${MEASURE_LABELS[t.measure]} at ${t.score}`}
            />
          ))}
      </div>
    </div>
  );
}
