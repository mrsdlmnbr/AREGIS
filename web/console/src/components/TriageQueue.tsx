// Severity-ranked, each alert carrying its FUSION RECEIPT: the exact terms,
// weights and contributions that produced the number. The receipt is the
// feature (spec §9.6) — an operator who cannot see WHY will not trust the
// system, and an insurer will not accept it.

import type { AlertView } from "../types";

export function TriageQueue({
  alerts,
  selected,
  onSelect,
}: {
  alerts: AlertView[];
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  const ranked = [...alerts].sort((a, b) => b.severity - a.severity);
  return (
    <div className="triage">
      <h2>Triage</h2>
      {ranked.length === 0 && <div className="empty">No open alerts — verified quiet.</div>}
      {ranked.map((a) => (
        <div
          key={a.id}
          className={`alert sev${a.severity} ${selected === a.id ? "selected" : ""}`}
          onClick={() => onSelect(a.id)}
        >
          <div className="alert-head">
            <span className={`sev-pill sev${a.severity}`}>SEV {a.severity}</span>
            <span className="alert-id">{a.id}</span>
            <span className="alert-at">{a.at}</span>
          </div>
          {selected === a.id && <FusionReceipt alert={a} />}
        </div>
      ))}
    </div>
  );
}

function FusionReceipt({ alert }: { alert: AlertView }) {
  return (
    <table className="receipt">
      <thead>
        <tr>
          <th>term</th>
          <th>input</th>
          <th>w</th>
          <th>Δ</th>
        </tr>
      </thead>
      <tbody>
        {alert.receipt.map((t) => (
          <tr key={t.name}>
            <td>{t.name}</td>
            <td className="receipt-input">{t.input}</td>
            <td>{t.weight.toFixed(1)}</td>
            <td className={t.contribution < 0 ? "neg" : ""}>{t.contribution.toFixed(2)}</td>
          </tr>
        ))}
        <tr className="receipt-total">
          <td>score</td>
          <td className="receipt-input">{alert.polNote}</td>
          <td />
          <td>{alert.score.toFixed(2)}</td>
        </tr>
      </tbody>
    </table>
  );
}
