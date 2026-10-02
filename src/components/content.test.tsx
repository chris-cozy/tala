import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  AnswerComparison,
  displayDoc,
  extractText,
  hasContent,
  textDoc,
} from "./RichContent";
import { CardPreview } from "../pages/Editor";
import { TalaContext, type TalaContextValue } from "../lib/context";
import { parseSteps, formatSteps } from "./SchedulingSettings";
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: vi.fn(),
  convertFileSrc: (path: string) =>
    `asset://localhost/${encodeURIComponent(path)}`,
}));

const context = {
  data: { mediaDir: "/isolated/media", preferences: { audioAutoplay: false } },
} as TalaContextValue;

describe("card content and preview", () => {
  it.each(["normal", "reversed", "typed"] as const)(
    "previews both sides of a %s card without scheduling",
    (behavior) => {
      const close = vi.fn();
      const { unmount } = render(
        <TalaContext.Provider value={context}>
          <CardPreview
            front={textDoc("Question")}
            back={textDoc("Answer")}
            behavior={behavior}
            onClose={close}
          />
        </TalaContext.Provider>,
      );
      expect(
        screen.getByText(behavior === "reversed" ? "Answer" : "Question"),
      ).toBeInTheDocument();
      if (behavior === "typed")
        fireEvent.change(screen.getByLabelText("Preview answer"), {
          target: { value: "anser" },
        });
      fireEvent.click(screen.getByRole("button", { name: "Reveal answer" }));
      expect(
        screen.getByText(behavior === "reversed" ? "Question" : "Answer"),
      ).toBeInTheDocument();
      if (behavior === "typed")
        expect(screen.getByText("anser")).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Back to editor" }));
      expect(close).toHaveBeenCalledOnce();
      unmount();
    },
  );

  it("keeps stable media identifiers while resolving local display URLs", () => {
    const doc = {
      type: "doc",
      content: [{ type: "image", attrs: { mediaId: "abc.png" } }],
    };
    const displayed = displayDoc(doc, "/isolated/media");
    expect(displayed.content?.[0].attrs?.mediaId).toBe("abc.png");
    expect(displayed.content?.[0].attrs?.src).toContain("abc.png");
    expect(doc.content[0].attrs).not.toHaveProperty("src");
    expect(hasContent(doc)).toBe(true);
    expect(hasContent(textDoc("   "))).toBe(false);
    expect(extractText(textDoc("A question"))).toBe("A question");
    expect(
      extractText({
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [
              { type: "text", text: "What is " },
              { type: "text", text: "cell", marks: [{ type: "bold" }] },
              { type: "text", text: " biology?" },
            ],
          },
        ],
      }),
    ).toBe("What is cell biology?");
  });

  it("shows differences without assigning an automatic grade", () => {
    render(<AnswerComparison answer="  café " expected="café" />);
    expect(screen.getByLabelText("Answer differences")).toHaveTextContent(
      "café",
    );
    expect(
      screen.getByText("Use your own judgment when grading recall."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});

describe("learning step editor", () => {
  it("round-trips seconds, minutes, hours, days and empty steps", () => {
    const steps = [30, 60, 600, 3600, 86400];
    expect(parseSteps(formatSteps(steps))).toEqual(steps);
    expect(parseSteps("1 10")).toEqual([60, 600]);
    expect(parseSteps("")).toEqual([]);
    expect(parseSteps("0.5m, 1h")).toEqual([30, 3600]);
  });
  it.each(["0s", "10m 1m", "8d", "hello", "1m 1m"])(
    "rejects unsafe or ambiguous steps: %s",
    (value) => {
      expect(() => parseSteps(value)).toThrow();
    },
  );
});
