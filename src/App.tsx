import { useState } from "react";
import { Sidebar, type View } from "./components/nav/Sidebar";
import { useDataChangedEvents } from "./hooks/useDataChangedEvents";
import { ProjectsView } from "./views/ProjectsView";
import { SessionsView } from "./views/SessionsView";
import { TimelineView } from "./views/TimelineView";
import { ComingSoonView } from "./views/ComingSoonView";

function App() {
  const [activeView, setActiveView] = useState<View>("projects");

  useDataChangedEvents();

  return (
    <div className="app-shell">
      <Sidebar active={activeView} onSelect={setActiveView} footer="Relay v0.1.0" />
      <main className="app-main">
        {activeView === "projects" && <ProjectsView />}
        {activeView === "sessions" && <SessionsView />}
        {activeView === "timeline" && <TimelineView />}
        {activeView === "connections" && <ComingSoonView title="Connections" />}
        {activeView === "agent-manager" && <ComingSoonView title="Agent Manager" />}
      </main>
    </div>
  );
}

export default App;
