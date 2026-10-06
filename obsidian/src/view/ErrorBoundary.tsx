import { Component, type ReactNode } from "react";

/** Shows an error (and a way back) instead of an empty sidebar if rendering fails */
export class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error): void {
    console.error("topos: the verse search sidebar failed", error);
  }

  render(): ReactNode {
    if (!this.state.error) return this.props.children;
    return (
      <div className="topos-crash">
        <p>The verse search sidebar ran into an error:</p>
        <pre>{String(this.state.error.stack ?? this.state.error)}</pre>
        <button onClick={() => this.setState({ error: null })}>Reload the sidebar</button>
      </div>
    );
  }
}
