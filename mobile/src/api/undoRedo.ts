// Undo/redo stack (50 steps). Covers task CRUD, schedule operations, habit CRUD.
// Sync operations are NOT included.

type UndoableAction = {
  description: string;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
};

const MAX_HISTORY = 50;

class UndoRedoManager {
  private undoStack: UndoableAction[] = [];
  private redoStack: UndoableAction[] = [];
  private listeners: Set<() => void> = new Set();

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

  push(action: UndoableAction) {
    this.undoStack.push(action);
    if (this.undoStack.length > MAX_HISTORY) {
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
