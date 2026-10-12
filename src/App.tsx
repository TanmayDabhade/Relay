import { useState } from "react";
import { Sidebar, type View } from "./components/nav/Sidebar";
import { ActiveProjectProvider } from "./hooks/ActiveProjectProvider";
import { useActiveProject } from "./hooks/useActiveProject";
import { useDataChangedEvents } from "./hooks/useDataChangedEvents";
import { DashboardView } from "./views/DashboardView";
import { ProjectsView } from "./views/ProjectsView";
import { ReportView } from "./views/ReportView";
import { TimelineView } from "./views/TimelineView";
import { AgentViewer } from "./views/AgentViewer";
import { ConnectionsView } from "./views/ConnectionsView";
import { SettingsView } from "./views/SettingsView";
import { WorkspaceView } from "./views/WorkspaceView";

interface AgentViewerTarget {
  taskId: string;
  runId: string;
  startedAt: number;
  /** The task's project. When it isn't the active one, the workspace switches to it so the
   * task isn't filtered out of Agent work. */
  projectId?: string;
}

function AppShell() {
  const [activeView, setActiveView] = useState<View>("dashboard");
  const [agentViewerTarget, setAgentViewerTarget] = useState<AgentViewerTarget | null>(null);
  const { activeProjectId, setActiveProjectId } = useActiveProject();

  useDataChangedEvents();

  function openAgentWork(target?: AgentViewerTarget) {
    if (target) {
      setAgentViewerTarget(target);
      if (target.projectId && activeProjectId !== null && target.projectId !== activeProjectId) {
        setActiveProjectId(target.projectId);
      }
    }
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
        {activeView === "board" && (
          <WorkspaceView onOpenProjects={() => setActiveView("projects")} />
        )}
        {activeView === "projects" && (
          <ProjectsView onOpenWorkspace={() => setActiveView("board")} />
        )}
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

function App() {
  return (
    <ActiveProjectProvider>
      <AppShell />
    </ActiveProjectProvider>
  );
}

export default App;
