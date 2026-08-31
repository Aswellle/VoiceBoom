/**
 * useAppStore behavioural tests.
 *
 * Covers the load-bearing logic that has no UI of its own:
 *   - M8 re-entrant guard on loadSettings (the infinite-loop fix)
 *   - settings round-trip: updateSettings persists + applies maxChars
 *   - addSegment enforces maxChars and clears the partial on a final result
 *   - applyMaxChars keeps dropping oldest segments until under budget
 *   - toast auto-dismisses after a length-scaled delay
 *
 * The Tauri layer is mocked in src/test/setup.ts: `invoke` writes to an
 * in-memory map, so these are pure store-logic tests with no desktop runtime.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { act } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../stores/useAppStore";

/** Advance fake timers and flush microtasks so async store work settles. */
async function flush(ms = 0) {
  await vi.advanceTimersByTimeAsync(ms);
}

beforeEach(() => {
  vi.useFakeTimers();
  // Start every test from a known clean state.
  useAppStore.setState({
    status: "idle",
    segments: [],
    currentPartial: "",
    settingsLoaded: false,
    shortcutRegistered: false,
    toastMessage: "",
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("loadSettings (M8 re-entrant guard)", () => {
  it("loads persisted settings once and exposes them on the store", async () => {
    // Pre-seed the fake SQLite via a save, then load.
    await act(async () => {
      useAppStore.getState().updateSettings({ engine: "deepgram" });
    });

    await act(async () => {
      await useAppStore.getState().loadSettings();
    });

    expect(useAppStore.getState().settings.engine).toBe("deepgram");
    expect(useAppStore.getState().settingsLoaded).toBe(true);
  });

  it("guards against re-entrant calls — second load after completion is a no-op", async () => {
    const spy = invoke;

    // First call performs the real load.
    await act(async () => {
      await useAppStore.getState().loadSettings();
    });
    const firstCallCount = spy.mock.calls.filter(
      ([cmd]: [string]) => cmd === "get_settings"
    ).length;

    // Simulate the engine:switched -> loadSettings loop that previously caused
    // React error #185. With the M8 guard, these must not hit the DB again.
    await act(async () => {
      await Promise.all([
        useAppStore.getState().loadSettings(),
        useAppStore.getState().loadSettings(),
        useAppStore.getState().loadSettings(),
      ]);
    });
    const totalCalls = spy.mock.calls.filter(
      ([cmd]: [string]) => cmd === "get_settings"
    ).length;

    // Exactly one DB read across the initial load + the re-entrant storm.
    expect(firstCallCount).toBe(1);
    expect(totalCalls).toBe(1);
    expect(useAppStore.getState().settingsLoaded).toBe(true);
  });

  it("sets settingsLoaded even when the load fails (does not loop forever)", async () => {
    // @ts-expect-error mocked — force a rejection
    invoke.mockRejectedValueOnce(new Error("db offline"));

    await act(async () => {
      await expect(useAppStore.getState().loadSettings()).resolves.toBeUndefined();
    });

    // The guard relies on the `finally` block flipping this flag; without it a
    // later engine:switched event would retry endlessly.
    expect(useAppStore.getState().settingsLoaded).toBe(true);
  });
});

describe("updateSettings persistence + maxChars side-effect", () => {
  it("persists each changed key via save_settings", async () => {
    const spy = invoke;

    await act(async () => {
      useAppStore.getState().updateSettings({ fontSize: 28, opacity: 0.9 });
    });

    const saveCalls = spy.mock.calls.filter(
      ([cmd]: [string]) => cmd === "save_settings"
    );
    expect(saveCalls).toHaveLength(2);
    expect(saveCalls).toEqual(
      expect.arrayContaining([
        expect.arrayContaining(["save_settings", { key: "fontSize", value: "28" }]),
        expect.arrayContaining(["save_settings", { key: "opacity", value: "0.9" }]),
      ])
    );
    expect(useAppStore.getState().settings.fontSize).toBe(28);
  });

  it("re-runs applyMaxChars when maxChars changes", async () => {
    await act(async () => {
      useAppStore.getState().addSegment({
        id: "s1",
        text: "一二三四五六七八九十", // 10 chars
        isFinal: true,
        timestamp: 1,
      });
      useAppStore.getState().addSegment({
        id: "s2",
        text: "ABCDEFGHIJ", // 10 chars
        isFinal: true,
        timestamp: 2,
      });
    });
    expect(useAppStore.getState().segments).toHaveLength(2);

    await act(async () => {
      useAppStore.getState().updateSettings({ maxChars: 15 });
    });

    // Total 20 chars > 15 budget → oldest segment(s) dropped. Keeps >= 1.
    const segs = useAppStore.getState().segments;
    const total = segs.reduce((n, s) => n + s.text.length, 0);
    expect(total).toBeLessThanOrEqual(15);
    expect(segs.length).toBeGreaterThanOrEqual(1);
  });
});

describe("addSegment + applyMaxChars budgeting", () => {
  it("clears the currentPartial when a final segment arrives", async () => {
    await act(async () => {
      useAppStore.getState().updatePartial("不完整的中间结果");
      useAppStore.getState().addSegment({
        id: "final",
        text: "完整的最终结果",
        isFinal: true,
        timestamp: 1,
      });
    });

    expect(useAppStore.getState().currentPartial).toBe("");
  });

  it("drops oldest segments first and never removes the last remaining one", async () => {
    await act(async () => {
      useAppStore.getState().addSegment({
        id: "old",
        text: "这很长的一段文字内容",
        isFinal: true,
        timestamp: 1,
      });
    });
    await act(async () => {
      useAppStore
        .getState()
        .applyMaxChars(5); // budget smaller than the single segment
    });

    const segs = useAppStore.getState().segments;
    // m6 fix: keep at least one segment even when over budget.
    expect(segs).toHaveLength(1);
    expect(segs[0].id).toBe("old");
  });
});

describe("toast auto-dismiss", () => {
  it("clears the toast after a length-scaled delay", async () => {
    await act(async () => {
      useAppStore.getState().showToast("复制失败");
    });
    expect(useAppStore.getState().toastMessage).toBe("复制失败");

    // Base duration is clamped to >= 4s for short messages.
    await act(async () => {
      await flush(4000);
    });
    expect(useAppStore.getState().toastMessage).toBe("");
  });

  it("a new toast resets the dismissal timer (does not get cut short)", async () => {
    await act(async () => {
      useAppStore.getState().showToast("第一条提示");
    });
    // Advance 3s (would not yet dismiss the first toast at 4s clamp).
    await act(async () => {
      await flush(3000);
    });
    // Post a second toast — must cancel the first timer.
    await act(async () => {
      useAppStore.getState().showToast("第二条更长的提示内容");
    });

    // At the original 4s mark the FIRST toast would have cleared, but the new
    // toast must still be showing because its timer was just (re)started.
    await act(async () => {
      await flush(1000); // total 4s from start, but new timer began at 3s
    });
    expect(useAppStore.getState().toastMessage).toBe("第二条更长的提示内容");

    // The longer message uses ~60ms/char: 9 chars → ~540ms, clamped to 4s.
    await act(async () => {
      await flush(4000);
    });
    expect(useAppStore.getState().toastMessage).toBe("");
  });
});
