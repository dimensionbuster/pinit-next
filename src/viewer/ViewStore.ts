import type { ViewState } from "../ipc.ts";

type Listener = (v: ViewState) => void;

/** Holds the window's view state; persists (debounced) through the injected sink. */
export class ViewStore {
  private listeners: Listener[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private view: ViewState,
    private readonly persist: (v: ViewState) => void,
    private readonly debounceMs = 300,
  ) {}

  get(): ViewState {
    return this.view;
  }

  onChange(fn: Listener): void {
    this.listeners.push(fn);
  }

  set(next: ViewState): void {
    this.view = next;
    for (const l of this.listeners) l(next);
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.persist(this.view), this.debounceMs);
  }

  update(fn: (v: ViewState) => ViewState): void {
    this.set(fn(this.view));
  }
}
