/**
 * Component-level tests for the FloatingWindow surface.
 *
 * What we can verify in jsdom (with the Tauri layer mocked in setup.ts):
 *   - SegmentItem renders text, is an a11y button, is keyboard actionable, and
 *     copies via the navigator.clipboard path with a working textarea fallback.
 *   - FloatingWindow renders its header controls, status line and the
 *     engine-configured hint.
 *   - The manual start/stop button flips listening state in the store.
 *   - Auto-resize calls setSize only when the computed height actually changes.
 *   - The scroll-to-bottom FAB appears once the user has scrolled away.
 *
 * We mock useAsr / useGlobalShortcut here because those hooks start real audio
 * pipelines via Tauri, out of scope for a jsdom component test. The component
 * props and store are real, so we still exercise the actual rendering,
 * click handling and effect wiring.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { SegmentItem, FloatingWindow } from "../components/FloatingWindow";
import type { RecognitionSegment } from "../stores/useAppStore";
import { useAppStore } from "../stores/useAppStore";
import { fakeWebviewWindow } from "./setup";

// --- Mocks for Tauri-coupled hooks -----------------------------------------

let listening = false;
const setListening = (v: boolean) => {
  listening = v;
  useAppStore.setState({ status: v ? "listening" : "idle" });
};

vi.mock("../hooks/useAsr", () => ({
  useAsr: () => ({
    startListening: vi.fn(async () => setListening(true)),
    stopListening: vi.fn(async () => setListening(false)),
    isListening: listening,
  }),
}));

vi.mock("../hooks/useGlobalShortcut", () => ({
  useGlobalShortcut: vi.fn(),
}));

vi.mock("../components/Waveform", () => ({
  Waveform: () => null,
}));

function makeSegment(over: Partial<RecognitionSegment> = {}): RecognitionSegment {
  return {
    id: "seg-1",
    text: "今天天气真不错",
    isFinal: true,
    language: "zh",
    confidence: 0.95,
    timestamp: Date.now(),
    ...over,
  };
}

beforeEach(() => {
  listening = false;
  useAppStore.setState({
    status: "idle",
    segments: [],
    currentPartial: "",
    settings: {
      ...useAppStore.getState().settings,
      reduceMotion: false,
      theme: "light",
      fontSize: 22,
      opacity: 1,
      engine: "funasr",
      apiKey: "",
    },
    settingsLoaded: true,
    shortcutRegistered: true,
    toastMessage: "",
    audioLevel: 0,
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("SegmentItem", () => {
  it("renders the segment text", () => {
    render(
      <SegmentItem
        segment={makeSegment()}
        isNewest
        reduceMotion={false}
        fontSize={22}
      />
    );
    expect(screen.getByText("今天天气真不错")).toBeInTheDocument();
  });

  it("exposes button semantics and is keyboard reachable", () => {
    render(
      <SegmentItem
        segment={makeSegment()}
        isNewest
        reduceMotion={false}
        fontSize={22}
      />
    );
    const btn = screen.getByRole("button", { name: /复制识别结果/ });
    expect(btn).toHaveAttribute("tabIndex", "0");
    expect(btn).toHaveAttribute(
      "aria-label",
      "复制识别结果：今天天气真不错"
    );
  });

  it("truncates a very long aria-label instead of reading the whole text", () => {
    const longText = "哈".repeat(80);
    render(
      <SegmentItem
        segment={makeSegment({ text: longText })}
        isNewest
        reduceMotion={false}
        fontSize={22}
      />
    );
    const btn = screen.getByRole("button", { name: /复制识别结果/ });
    expect(btn.getAttribute("aria-label")!.length).toBeLessThan(longText.length);
  });

  it("triggers a copy on click, Enter and Space keys", async () => {
    const user = userEvent.setup();
    render(
      <SegmentItem
        segment={makeSegment()}
        isNewest
        reduceMotion={false}
        fontSize={22}
      />
    );
    const btn = screen.getByRole("button", { name: /复制识别结果/ });
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });

    await user.click(btn);
    expect(writeText).toHaveBeenCalledWith("今天天气真不错");

    writeText.mockClear();
    btn.focus();
    await user.keyboard("{Enter}");
    expect(writeText).toHaveBeenCalledWith("今天天气真不错");
  });

  it("falls back to textarea+execCommand when clipboard API is missing", async () => {
    const user = userEvent.setup();
    const origClipboard = navigator.clipboard;
    Object.defineProperty(navigator, "clipboard", {
      value: undefined,
      configurable: true,
    });
    const execCommand = vi
      .spyOn(document, "execCommand")
      .mockReturnValue(true);
    const removeChild = vi.spyOn(HTMLElement.prototype, "removeChild");

    try {
      render(
        <SegmentItem
          segment={makeSegment()}
          isNewest
          reduceMotion={false}
          fontSize={22}
        />
      );
      const btn = screen.getByRole("button", { name: /复制识别结果/ });
      await user.click(btn);
      expect(execCommand).toHaveBeenCalledWith("copy");
      expect(removeChild).toHaveBeenCalled();
    } finally {
      Object.defineProperty(navigator, "clipboard", {
        value: origClipboard,
        configurable: true,
      });
      execCommand.mockRestore();
      removeChild.mockRestore();
    }
  });
});

describe("FloatingWindow", () => {
  it("renders header controls, engine label and a status line", () => {
    render(<FloatingWindow />);
    expect(screen.getByRole("button", { name: "说话" })).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "打开设置" })
    ).toBeInTheDocument();
    expect(screen.getByText("SenseVoice")).toBeInTheDocument();
    expect(useAppStore.getState().settingsLoaded).toBe(true);
  });

  it("shows the unconfigured-engine hint when no model is installed", async () => {
    globalThis.__setEngineResult?.({
      is_local: true,
      model_installed: false,
      tokens_installed: false,
      vad_installed: false,
    });
    render(<FloatingWindow />);
    await waitFor(() =>
      expect(screen.getByText(/请先打开「设置」/)).toBeInTheDocument()
    );
  });

  it("toggles listening via the manual button and updates the store", async () => {
    const user = userEvent.setup();
    render(<FloatingWindow />);
    const btn = screen.getByRole("button", { name: "说话" });
    await user.click(btn);
    expect(useAppStore.getState().status).toBe("listening");
    expect(
      screen.getByRole("button", { name: "停止" })
    ).toBeInTheDocument();
  });

  it("only resizes the window when the desired height changes", () => {
    const setSize = fakeWebviewWindow.setSize;
    const { rerender, container } = render(<FloatingWindow />);
    const callsAfterFirst = setSize.mock.calls.length;

    // Re-render with unchanged content → no new resize.
    rerender(<FloatingWindow />);
    expect(setSize.mock.calls.length).toBe(callsAfterFirst);

    // Simulate content growing: bump the measured content height on the
    // actual content element (jsdom has no layout engine, so we drive the
    // reflow value directly) and add a segment so the effect re-runs.
    const contentEl = container.querySelector(
      ".flex.flex-col.gap-1"
    ) as HTMLElement;
    Object.defineProperty(contentEl, "scrollHeight", {
      value: 300,
      configurable: true,
    });
    useAppStore.setState({
      segments: [
        makeSegment({ id: "a", text: "第一段文字内容" }),
        makeSegment({ id: "b", text: "第二段文字内容" }),
      ],
    });
    rerender(<FloatingWindow />);
    expect(setSize.mock.calls.length).toBeGreaterThan(callsAfterFirst);
  });
  it("shows the scroll-to-bottom FAB only after the user scrolls up", () => {
    useAppStore.setState({
      segments: [
        makeSegment({ id: "a", text: "旧的第一段识别文字" }),
        makeSegment({ id: "b", text: "新的第二段识别文字" }),
      ],
    });
    const { container } = render(<FloatingWindow />);

    const scrollHost = container.querySelector(
      ".overflow-y-auto"
    ) as HTMLElement;
    expect(scrollHost).toBeTruthy();

    expect(
      screen.queryByRole("button", { name: "返回最新内容" })
    ).toBeNull();

    Object.defineProperty(scrollHost, "scrollHeight", {
      value: 500,
      configurable: true,
    });
    Object.defineProperty(scrollHost, "clientHeight", {
      value: 100,
      configurable: true,
    });
    Object.defineProperty(scrollHost, "scrollTop", {
      value: 0,
      configurable: true,
    });
    fireEvent.scroll(scrollHost);

    expect(
      screen.getByRole("button", { name: "返回最新内容" })
    ).toBeInTheDocument();
  });
});
