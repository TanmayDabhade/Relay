import { Button } from "./ui/Button";
import { openUrl } from "../lib/tauri";
import "./UpgradeBanner.css";

const LANDING_URL: string | undefined = import.meta.env.VITE_LANDING_URL;

/** Free-plan "N hidden — upgrade" bar shown above the Projects and Sessions lists. The caller
 * decides whether to render it (only when the plan is free and something is actually hidden —
 * see `plan_gating_status`); this just presents the message and the upgrade action, mirroring
 * ProfileView's Upgrade button (same `VITE_LANDING_URL` gating so a build without a landing
 * URL disables the button rather than opening nothing). */
export function UpgradeBanner({ message }: { message: string }) {
  return (
    <div className="upgrade-banner" role="status">
      <span className="upgrade-banner-text">{message}</span>
      <Button
        variant="secondary"
        disabled={!LANDING_URL}
        title={LANDING_URL ? undefined : "Upgrade isn't available yet"}
        onClick={() => LANDING_URL && openUrl(LANDING_URL)}
      >
        Upgrade
      </Button>
    </div>
  );
}
