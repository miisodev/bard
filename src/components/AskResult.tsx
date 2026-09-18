import { Copy, X } from "lucide-react";
import { api } from "../store";

export default function AskResult({
  prompt,
  result,
  busy,
  onDismiss,
}: {
  prompt: string;
  result: string;
  busy: boolean;
  onDismiss: () => void;
}) {
  return (
    <div className="ask-result">
      <div className="q">{prompt}</div>
      <div className={`a${busy ? " busy" : ""}`}>
        {busy ? "Thinking…" : result}
      </div>
      <div className="row">
        {!busy ? (
          <button
            className="mini-btn"
            onClick={() => {
              void api.copyText(result);
              onDismiss();
            }}
          >
            <Copy size={11} /> Copy
          </button>
        ) : null}
        <button className="mini-btn" onClick={onDismiss}>
          <X size={11} /> Dismiss
        </button>
      </div>
    </div>
  );
}