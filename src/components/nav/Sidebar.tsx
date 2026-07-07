import "./Sidebar.css";

export type View =
  | "projects"
  | "sessions"
  | "timeline"
  | "connections"
  | "agent-manager";

const NAV_ITEMS: { id: View; label: string; icon: string }[] = [
  { id: "projects", label: "Projects", icon: "⊞" },
  { id: "sessions", label: "Sessions", icon: "◷" },
  { id: "timeline", label: "Timeline", icon: "⋮" },
  { id: "connections", label: "Connections", icon: "⟳" },
  { id: "agent-manager", label: "Agent Manager", icon: "▶" },
];

interface SidebarProps {
  active: View;
  onSelect: (view: View) => void;
  footer: string;
}

export function Sidebar({ active, onSelect, footer }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">Manageai</div>
      <div className="sidebar-section-label">Workspace</div>
      <nav>
        {NAV_ITEMS.map((item) => (
          <button
            key={item.id}
            className={`sidebar-nav-item${active === item.id ? " active" : ""}`}
            onClick={() => onSelect(item.id)}
          >
            <span className="sidebar-nav-icon">{item.icon}</span>
            {item.label}
          </button>
        ))}
      </nav>
      <div className="sidebar-footer">{footer}</div>
    </aside>
  );
}
