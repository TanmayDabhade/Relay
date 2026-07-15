import { useState } from "react";
import { Sidebar, type View } from "./components/nav/Sidebar";
import { useAuth } from "./hooks/useAuth";
import { useDataChangedEvents } from "./hooks/useDataChangedEvents";
import { AuthGate } from "./views/AuthGate";
import { DashboardView } from "./views/DashboardView";
import { ProjectsView } from "./views/ProjectsView";
import { ReportView } from "./views/ReportView";
import { SessionsView } from "./views/SessionsView";
import { TimelineView } from "./views/TimelineView";
import { ComingSoonView } from "./views/ComingSoonView";
import { ProfileView } from "./views/ProfileView";

function App() {
  const [activeView, setActiveView] = useState<View>("dashboard");
  const { status } = useAuth();

  useDataChangedEvents();

  // Hard gate: nothing below (dashboard, projects, sessions, local session data) is
  // reachable without a signed-in session — see AuthGate's doc comment for why this is a
  // gate rather than a nav item layered on top of an already-visible app.
  if (status !== "signed-in") {
    return <AuthGate />;
  }

  return (
    <div className="app-shell">
      <Sidebar active={activeView} onSelect={setActiveView} footer="Relay v0.1.0" />
      <main className="app-main">
        {activeView === "dashboard" && <DashboardView />}
        {activeView === "projects" && <ProjectsView />}
        {activeView === "sessions" && <SessionsView />}
        {activeView === "timeline" && <TimelineView />}
        {activeView === "report" && <ReportView />}
        {activeView === "connections" && <ComingSoonView title="Connections" />}
        {activeView === "agent-manager" && <ComingSoonView title="Agent Manager" />}
        {activeView === "profile" && <ProfileView />}
      </main>
    </div>
  );
}

export default App;
