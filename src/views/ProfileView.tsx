import { Button } from "../components/ui/Button";
import { Pill } from "../components/ui/Pill";
import { useAuth } from "../hooks/useAuth";
import { openUrl } from "../lib/tauri";
import "./ProfileView.css";

const LANDING_URL: string | undefined = import.meta.env.VITE_LANDING_URL;

// App.tsx never mounts this before "signed-in" (see AuthGate) — so this only ever renders
// the account panel, not a sign-in form.
export function ProfileView() {
  const { user, plan, signOut } = useAuth();
  if (!user) return null;

  return (
    <div className="profile-view">
      <div className="profile-view-topbar">
        <h1 className="view-topbar-title">Profile</h1>
      </div>

      <div className="dashboard-card profile-card">
        <div className="profile-signed-in">
          <div className="profile-row">
            <span className="profile-row-label">Email</span>
            <span className="profile-row-value">{user.email ?? "unknown"}</span>
          </div>
          <div className="profile-row">
            <span className="profile-row-label">Plan</span>
            <Pill variant="tag" tone={plan === "paid" ? "green" : "gray"}>
              {plan === "paid" ? "Paid" : "Free"}
            </Pill>
          </div>

          {plan === "free" && (
            <p className="profile-free-note">
              Free plan: up to 3 projects, 50 tracked sessions. AI summaries and Reports/export
              require a paid plan.
            </p>
          )}

          <div className="profile-actions">
            {plan === "free" && (
              <Button
                variant="secondary"
                disabled={!LANDING_URL}
                title={LANDING_URL ? undefined : "Upgrade isn't available yet"}
                onClick={() => LANDING_URL && openUrl(LANDING_URL)}
              >
                Upgrade
              </Button>
            )}
            <Button variant="secondary" onClick={signOut}>
              Sign out
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
