import { useState } from "react";
import { Sidebar, type View } from "./components/nav/Sidebar";
import { useDataChangedEvents } from "./hooks/useDataChangedEvents";
import { DashboardView } from "./views/DashboardView";
import { ProjectsView } from "./views/ProjectsView";
import { ReportView } from "./views/ReportView";
import { TimelineView } from "./views/TimelineView";
import { AgentViewer } from "./views/AgentViewer";
import { ConnectionsView } from "./views/ConnectionsView";
import { SettingsView } from "./views/SettingsView";

interface AgentViewerTarget {
  taskId: string;
  runId: string;
  startedAt: number;
}

function App() {
  const [activeView, setActiveView] = useState<View>("dashboard");
  const [agentViewerTarget, setAgentViewerTarget] = useState<AgentViewerTarget | null>(null);

  useDataChangedEvents();

  function openAgentWork(target?: AgentViewerTarget) {
    if (target) setAgentViewerTarget(target);
    setActiveView("agent-viewer");
  }

  return (
    <div className="app-shell">
      <Sidebar active={activeView} onSelect={setActiveView} footer="Relay v0.1.0" />
      <main className="app-main">
        {activeView === "dashboard" && (
          <DashboardView
            onOpenAgentWork={openAgentWork}
          />
        )}
        {activeView === "projects" && <ProjectsView />}
        {activeView === "timeline" && <TimelineView />}
        {activeView === "report" && <ReportView />}
        {activeView === "connections" && <ConnectionsView />}
        {activeView === "settings" && <SettingsView />}
        {activeView === "agent-viewer" && (
          <AgentViewer
            initialTaskId={agentViewerTarget?.taskId}
            initialRunId={agentViewerTarget?.runId}
            initialStartedAt={agentViewerTarget?.startedAt}
            onOpenHome={() => setActiveView("dashboard")}
            onTargetChange={setAgentViewerTarget}
          />
        )}
      </main>
    </div>
  );
}

export default App;
