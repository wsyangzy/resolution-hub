import { fireEvent, render, screen } from "@testing-library/react";
import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";

import { MicroButton } from "./MicroButton";

describe("MicroButton", () => {
  it("tracks the pointer glow and forwards its ref and native events", () => {
    const ref = createRef<HTMLButtonElement>();
    const onClick = vi.fn();
    const onPointerMove = vi.fn();
    render(
      <MicroButton ref={ref} onClick={onClick} onPointerMove={onPointerMove}>
        执行操作
      </MicroButton>,
    );

    const button = screen.getByRole("button", { name: "执行操作" });
    vi.spyOn(button, "getBoundingClientRect").mockReturnValue({
      left: 10,
      top: 20,
      right: 130,
      bottom: 60,
      width: 120,
      height: 40,
      x: 10,
      y: 20,
      toJSON: () => ({}),
    });

    fireEvent.pointerMove(button, { clientX: 37, clientY: 52 });
    fireEvent.click(button);

    expect(button.style.getPropertyValue("--micro-pointer-x")).toBe("27px");
    expect(button.style.getPropertyValue("--micro-pointer-y")).toBe("32px");
    expect(onPointerMove).toHaveBeenCalledTimes(1);
    expect(onClick).toHaveBeenCalledTimes(1);
    expect(ref.current).toBe(button);
  });

  it("exposes a stable loading state", () => {
    render(<MicroButton loading>保存</MicroButton>);

    const button = screen.getByRole("button", { name: "正在处理..." });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-busy", "true");
  });
});
