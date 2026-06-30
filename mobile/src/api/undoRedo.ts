// Undo/redo stack. Covers task CRUD, schedule operations, habit CRUD.
// Sync operations are NOT included.
// Max history is configurable via setMaxHistory() (default 50).

type UndoableAction = {
  description: string;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
};

export const DEFAULT_MAX_HISTORY = 50;

class UndoRedoManager {
  private undoStack: UndoableAction[] = [];
  private redoStack: UndoableAction[] = [];
  private listeners: Set<() => void> = new Set();
  private maxHistory = DEFAULT_MAX_HISTORY;

  private notify() {
    this.listeners.forEach((l) => l());
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  canUndo(): boolean {
    return this.undoStack.length > 0;
  }

  canRedo(): boolean {
    return this.redoStack.length > 0;
  }

  getMaxHistory(): number {
    return this.maxHistory;
  }

  setMaxHistory(n: number) {
    if (!Number.isFinite(n) || n <= 0) return;
    this.maxHistory = Math.floor(n);
    // Trim existing stacks to the new limit
    while (this.undoStack.length > this.maxHistory) {
      this.undoStack.shift();
    }
    while (this.redoStack.length > this.maxHistory) {
      this.redoStack.shift();
    }
    this.notify();
  }

  push(action: UndoableAction) {
    this.undoStack.push(action);
    if (this.undoStack.length > this.maxHistory) {
      this.undoStack.shift();
    }
    this.redoStack = [];
    this.notify();
  }

  async undo(): Promise<void> {
    const action = this.undoStack.pop();
    if (!action) return;
    await action.undo();
    this.redoStack.push(action);
    this.notify();
  }

  async redo(): Promise<void> {
    const action = this.redoStack.pop();
    if (!action) return;
    await action.redo();
    this.undoStack.push(action);
    this.notify();
  }

  clear() {
    this.undoStack = [];
    this.redoStack = [];
    this.notify();
  }
}

export const undoRedo = new UndoRedoManager();
