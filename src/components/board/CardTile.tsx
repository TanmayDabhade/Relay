import { agentMeta } from "../../lib/agents";
import { openUrl } from "../../lib/tauri";
import type { Card, CardShip, Session } from "../../lib/types";
import { prLabel, SHIP_LABELS } from "../../lib/ship";
import "./CardTile.css";

interface CardTileProps {
  card: Card;
  linkedSession: Session | null;
  ship?: CardShip | null;
  onClick: () => void;
  onDragStart: (cardId: string) => void;
}

export function CardTile({ card, linkedSession, ship, onClick, onDragStart }: CardTileProps) {
  return (
    <div
      className="card-tile"
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData("text/plain", card.id);
        e.dataTransfer.effectAllowed = "move";
        onDragStart(card.id);
      }}
      onClick={onClick}
    >
      <div className="card-tile-title">{card.title}</div>
      {card.description ? <div className="card-tile-description">{card.description}</div> : null}
      {linkedSession ? (
        <div className="card-tile-session">
          <span className={`card-tile-session-dot card-tile-session-dot-${linkedSession.status}`} />
          {agentMeta(linkedSession.agent).icon} {linkedSession.model ?? agentMeta(linkedSession.agent).label}
          {linkedSession.cost_usd > 0 ? ` · $${linkedSession.cost_usd.toFixed(2)}` : ""}
        </div>
      ) : null}
      {ship ? (
        ship.pr_url ? (
          <button
            type="button"
            className={`ship-badge is-${ship.ship_status}`}
            title={`${ship.branch} — open the pull request`}
            onClick={(event) => {
              event.stopPropagation();
              void openUrl(ship.pr_url!);
            }}
          >
            ↗ {prLabel(ship.pr_url)}
          </button>
        ) : (
          <span className={`ship-badge is-${ship.ship_status}`} title={ship.branch}>
            {SHIP_LABELS[ship.ship_status]}
          </span>
        )
      ) : null}
    </div>
  );
}
