import { act, cleanup, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useAudioPlayback } from "./AudioContent";

function PlaybackHarness({ playbackKey }: { playbackKey: string }) {
  const host = useRef<HTMLDivElement>(null);
  const error = useAudioPlayback(host, playbackKey, true);
  return (
    <div ref={host}>
      <audio aria-label="first clip" />
      <audio aria-label="second clip" />
      {error && <div role="status">{error}</div>}
    </div>
  );
}

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useAudioPlayback", () => {
  it("ignores a rejected play promise from a cancelled card", async () => {
    vi.useFakeTimers();
    const { container, rerender } = render(
      <PlaybackHarness playbackKey="front-one" />,
    );
    const [first] = container.querySelectorAll("audio");
    const second = container.querySelectorAll("audio")[1];
    let rejectPlay!: (reason?: unknown) => void;
    const play = vi.spyOn(first, "play").mockImplementation(
      () =>
        new Promise<void>((_resolve, reject) => {
          rejectPlay = reject;
        }),
    );
    const pause = vi.spyOn(first, "pause").mockImplementation(() => {});
    vi.spyOn(second, "pause").mockImplementation(() => {});

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50);
    });
    expect(play).toHaveBeenCalledOnce();

    rerender(<PlaybackHarness playbackKey="front-two" />);
    expect(pause).toHaveBeenCalled();
    await act(async () => {
      rejectPlay(new Error("aborted playback"));
      await Promise.resolve();
    });

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("plays clips in order and stops the queue on blur", async () => {
    vi.useFakeTimers();
    const { container } = render(<PlaybackHarness playbackKey="card-one" />);
    const [first, second] = container.querySelectorAll("audio");
    const firstPlay = vi.spyOn(first, "play").mockResolvedValue(undefined);
    const secondPlay = vi.spyOn(second, "play").mockResolvedValue(undefined);
    const firstPause = vi.spyOn(first, "pause").mockImplementation(() => {});
    const secondPause = vi.spyOn(second, "pause").mockImplementation(() => {});

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50);
    });
    expect(firstPlay).toHaveBeenCalledOnce();
    expect(secondPlay).not.toHaveBeenCalled();

    await act(async () => {
      first.dispatchEvent(new Event("ended"));
      await Promise.resolve();
    });
    expect(secondPlay).toHaveBeenCalledOnce();

    act(() => window.dispatchEvent(new Event("blur")));
    expect(firstPause).toHaveBeenCalled();
    expect(secondPause).toHaveBeenCalled();
  });

  it("shows a recovery message when the browser blocks playback", async () => {
    vi.useFakeTimers();
    const { container } = render(<PlaybackHarness playbackKey="card-two" />);
    const [first] = container.querySelectorAll("audio");
    vi.spyOn(first, "play").mockRejectedValue(new Error("blocked"));

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50);
      await Promise.resolve();
    });

    expect(screen.getByRole("status")).toHaveTextContent(
      "Auto-play was unavailable. Use the play or replay controls.",
    );
  });
});
