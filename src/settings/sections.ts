import { ipc, type Corner, type Flow, type ModeKind, type Theme } from "../ipc.ts";
import { clear, h } from "../ui/dom.ts";
import { icon } from "../ui/icons.ts";
import type { SettingsModel } from "./SettingsModel.ts";
import { card, field, segmented, toast, toggle } from "./widgets.ts";

/** A section owns one card and re-renders itself when the model changes. */
export interface Section {
  readonly el: HTMLElement;
}

abstract class BaseSection implements Section {
  readonly el = h("div", { class: "section-host" });
  constructor(protected readonly model: SettingsModel) {
    model.onChange(() => this.mount());
  }
  protected mount(): void {
    clear(this.el);
    this.el.append(this.build());
  }
  protected abstract build(): HTMLElement;
  init(): this {
    this.mount();
    return this;
  }
}

export class OpenSection extends BaseSection {
  private input = h("input", {
    class: "input",
    type: "url",
    placeholder: "이미지 주소 붙여넣기 (https://…)",
    "aria-label": "이미지 주소",
  });

  protected build(): HTMLElement {
    const submit = async () => {
      const text = this.input.value.trim();
      if (!text) return;
      try {
        await ipc.openUrlText(text);
        this.input.value = "";
      } catch (e) {
        toast(String(e));
      }
    };
    this.input.onkeydown = (e) => e.key === "Enter" && void submit();
    return card(
      "열기",
      h(
        "div",
        { class: "dropzone" },
        icon("image"),
        h("div", {}, h("div", { class: "h2" }, "이미지를 여기에 끌어다 놓기"), h("div", { class: "caption" }, "웹 이미지는 아래 \u2018브라우저 연동\u2019을 설치하면 우클릭으로 바로 열 수 있습니다")),
        h("button", { class: "btn", type: "button", onClick: () => void ipc.pickAndOpen().then(() => this.model.refresh()) }, icon("folder"), "파일 선택"),
      ),
      h("div", { class: "row" }, this.input, h("button", { class: "btn primary", type: "button", onClick: () => void submit() }, icon("link"), "열기")),
    );
  }
}

const GRID_PRESETS = [
  { label: "S", value: 200 },
  { label: "M", value: 300 },
  { label: "L", value: 440 },
];

export class ArrangeSection extends BaseSection {
  protected build(): HTMLElement {
    const s = this.model.settings;
    const m = this.model;
    const mode = field(
      "배치 방식",
      s.mode === "grid" ? "크기를 고정하고 줄 맞춰 채웁니다." : "크기를 조금씩 달리해 퍼즐처럼 채웁니다.",
      segmented<ModeKind>(
        [
          { value: "grid", label: "격자", icon: "grid" },
          { value: "jigsaw", label: "자율", icon: "jigsaw" },
        ],
        s.mode,
        (mode) => m.patch({ mode }),
        "배치 방식",
      ),
    );

    const size =
      s.mode === "grid"
        ? field(
            "창 크기",
            `줄 두께 ${s.grid_line} px`,
            h(
              "div",
              { class: "row" },
              ...GRID_PRESETS.map((p) =>
                h("button", { class: "chip", type: "button", "aria-pressed": s.grid_line === p.value, onClick: () => m.patch({ grid_line: p.value }) }, p.label),
              ),
              h("input", {
                class: "input num",
                type: "number",
                min: 80,
                max: 1600,
                value: s.grid_line,
                "aria-label": "줄 두께 직접 입력",
                onChange: (e) => m.patch({ grid_line: Number((e.target as HTMLInputElement).value) || s.grid_line }),
              }),
            ),
          )
        : h(
            "div",
            { class: "stack" },
            field("기준 크기", `${s.jigsaw_base} px`, rangeInput(80, 800, 10, s.jigsaw_base, (v) => m.patch({ jigsaw_base: v }, false), "기준 크기", (v) => `${v} px`)),
            field("크기 편차 (최소)", `${Math.round(s.jigsaw_min_scale * 100)} %`, rangeInput(30, 100, 5, Math.round(s.jigsaw_min_scale * 100), (v) => m.patch({ jigsaw_min_scale: v / 100 }, false), "최소 배율", (v) => `${v} %`)),
            field("크기 편차 (최대)", `${Math.round(s.jigsaw_max_scale * 100)} %`, rangeInput(100, 250, 5, Math.round(s.jigsaw_max_scale * 100), (v) => m.patch({ jigsaw_max_scale: v / 100 }, false), "최대 배율", (v) => `${v} %`)),
          );

    const anchor = field(
      "채우기 시작점",
      "이 모서리에서 시작해 반대쪽으로 채웁니다. 남는 빈틈은 아직 안 채운 쪽에 모입니다.",
      h(
        "div",
        { class: "row" },
        anchorPicker(s.corner, (corner) => m.patch({ corner })),
        segmented<Flow>(
          [
            { value: "rows", label: "가로로" },
            { value: "columns", label: "세로로" },
          ],
          s.flow,
          (flow) => m.patch({ flow }),
          "채우는 방향",
        ),
      ),
    );

    const display = field(
      "새 창을 띄울 화면",
      null,
      h(
        "select",
        {
          class: "input select",
          "aria-label": "화면 선택",
          onChange: (e) => {
            const v = (e.target as HTMLSelectElement).value;
            m.patch({ preferred_display: v === "auto" ? null : Number(v) });
          },
        },
        h("option", { value: "auto", selected: s.preferred_display === null }, "마우스가 있는 화면"),
        ...m.displays.map((d, i) =>
          h("option", { value: String(d.id), selected: s.preferred_display === d.id }, `${i + 1}번 화면${d.primary ? " (주)" : ""} · ${d.width}×${d.height} · ${Math.round(d.scale * 100)}%`),
        ),
      ),
    );

    const keep = field(
      "다시 정렬 시 현재 크기 유지",
      "켜면 창 크기는 그대로 두고 위치만 시작 모서리부터 다시 채웁니다.",
      toggle(s.relayout_keep_sizes, "다시 정렬 시 현재 크기 유지", (v) => m.patch({ relayout_keep_sizes: v })),
    );

    return card("배치", mode, size, anchor, display, keep);
  }
}

function rangeInput(
  min: number,
  max: number,
  step: number,
  value: number,
  onInput: (v: number) => void,
  label: string,
  fmt: (v: number) => string,
): HTMLElement {
  return h("input", {
    type: "range",
    class: "range",
    min,
    max,
    step,
    value,
    "aria-label": label,
    onInput: (e) => {
      const input = e.target as HTMLInputElement;
      const v = Number(input.value);
      onInput(v);
      const caption = input.closest(".field")?.querySelector(".caption");
      if (caption) caption.textContent = fmt(v); // update in place: re-rendering would cancel the drag
    },
  });
}

function anchorPicker(current: Corner, onPick: (c: Corner) => void): HTMLElement {
  const corners: { c: Corner; label: string }[] = [
    { c: "topleft", label: "왼쪽 위" },
    { c: "topright", label: "오른쪽 위" },
    { c: "bottomleft", label: "왼쪽 아래" },
    { c: "bottomright", label: "오른쪽 아래" },
  ];
  return h(
    "div",
    { class: "anchor-picker", role: "group", "aria-label": "시작 모서리" },
    ...corners.map(({ c, label }) =>
      h("button", { class: `corner ${c}`, type: "button", title: label, "aria-label": label, "aria-pressed": c === current, onClick: () => onPick(c) }),
    ),
  );
}

export class LayoutSection extends BaseSection {
  protected build(): HTMLElement {
    const n = this.model.openCount;
    const run = async (job: () => Promise<string | number | null>, ok: (r: string | number) => string) => {
      try {
        const r = await job();
        if (r !== null) toast(ok(r));
      } catch (e) {
        toast(String(e));
      }
      await this.model.refresh();
    };
    return card(
      "레이아웃",
      field(
        "저장 / 불러오기",
        "열려 있는 이미지와 위치·크기·보기 상태를 .pinit 파일로 저장하고, 나중에 불러옵니다.",
        h(
          "div",
          { class: "row" },
          h("button", { class: "btn", type: "button", disabled: n === 0, onClick: () => void run(() => ipc.exportLayout(), (p) => `저장했습니다: ${p}`) }, icon("download"), "저장"),
          h("button", { class: "btn", type: "button", onClick: () => void run(() => ipc.importLayout(), (c) => `${c}개 창을 불러옵니다`) }, icon("upload"), "불러오기"),
        ),
      ),
    );
  }
}

export class BrowserSection extends BaseSection {
  protected build(): HTMLElement {
    const connected = this.model.settings.bridge_connected;
    const step = (n: number, text: string) =>
      h("li", {}, h("span", { class: "step-n" }, String(n)), h("span", {}, text));
    return card(
      "브라우저 연동",
      h(
        "div",
        { class: "row between" },
        h("div", { class: "caption" }, "크롬·엣지·브라우저에서 웹페이지의 이미지를 마우스 오른쪽 버튼으로 눌러 바로 PinIt 창으로 띄웁니다."),
        h("span", { class: `status ${connected ? "ok" : ""}`, role: "status" }, h("i", {}), connected ? "연결됨" : "아직 연결 안 됨"),
      ),
      h(
        "ol",
        { class: "steps" },
        step(1, "아래 ‘확장 폴더 열기’를 누르면 폴더가 열립니다. (그 폴더 안에 manifest.json이 있습니다)"),
        step(2, "크롬 주소창에 chrome://extensions 를 입력하고, 오른쪽 위 ‘개발자 모드’를 켭니다."),
        step(3, "왼쪽 위 ‘압축해제된 확장 프로그램을 로드합니다’를 누르고 1번에서 열린 폴더를 선택합니다."),
        step(4, "설치되면 이 카드에 ‘연결됨’이 표시됩니다. 이제 웹페이지의 이미지 위에서 오른쪽 클릭 → ‘PinIt으로 열기’."),
      ),
      h(
        "div",
        { class: "row" },
        h("button", { class: "btn", type: "button", onClick: () => void ipc.revealExtension().catch((e) => toast(String(e))) }, icon("folder"), "확장 폴더 열기"),
        h("span", { class: "caption" }, "확장은 자동으로 설치되지 않습니다. 브라우저 정책상 한 번 직접 로드해야 합니다."),
      ),
    );
  }
}

export class AppearanceSection extends BaseSection {
  protected build(): HTMLElement {
    const s = this.model.settings;
    const m = this.model;
    return card(
      "화면",
      field(
        "테마",
        null,
        segmented<Theme>(
          [
            { value: "system", label: "시스템", icon: "monitor" },
            { value: "light", label: "라이트", icon: "sun" },
            { value: "dark", label: "다크", icon: "moon" },
          ],
          s.theme,
          (theme) => m.patch({ theme }),
          "테마",
        ),
      ),
      field(
        "이미지 뒤 배경",
        "이미지 밖 빈 영역과 줌아웃·패닝 때 보이는 배경입니다.",
        h(
          "div",
          { class: "row" },
          h("button", { class: "chip", type: "button", "aria-pressed": s.backdrop_color === "#000000", onClick: () => m.patch({ backdrop_color: "#000000" }) }, "검정"),
          h("button", { class: "chip", type: "button", "aria-pressed": s.backdrop_color === "#ffffff", onClick: () => m.patch({ backdrop_color: "#ffffff" }) }, "흰색"),
          h("input", {
            class: "color",
            type: "color",
            value: s.backdrop_color,
            "aria-label": "배경 색",
            onInput: (e) => m.patch({ backdrop_color: (e.target as HTMLInputElement).value }, false),
          }),
        ),
      ),
      field(
        "배경 투명도",
        `${Math.round(s.backdrop_opacity * 100)} %`,
        rangeInput(0, 100, 5, Math.round(s.backdrop_opacity * 100), (v) => m.patch({ backdrop_opacity: v / 100 }, false), "배경 투명도", (v) => `${v} %`),
      ),
      field("작업 표시줄에 이미지 창 표시", "다음에 여는 창부터 적용됩니다.", toggle(s.show_in_taskbar, "작업 표시줄에 표시", (v) => m.patch({ show_in_taskbar: v }))),
    );
  }
}

export class Header extends BaseSection {
  protected build(): HTMLElement {
    const n = this.model.openCount;
    const run = (p: Promise<unknown>) => void p.then(() => this.model.refresh());
    return h(
      "header",
      { class: "page-head" },
      h("div", { class: "brand" }, h("span", { class: "logo" }, icon("pin")), h("div", {}, h("h1", { class: "title" }, "PinIt"), h("div", { class: "caption" }, n === 0 ? "열린 이미지 없음" : `열린 이미지 ${n}개`))),
      h(
        "div",
        { class: "row" },
        h("button", { class: "btn", type: "button", disabled: n === 0, onClick: () => run(ipc.relayout()) }, icon("arrange"), "다시 정렬"),
        h("button", { class: "btn danger", type: "button", disabled: n === 0, onClick: () => run(ipc.closeAll()) }, icon("trash"), "모두 닫기"),
        h("button", { class: "btn ghost icon-only", type: "button", title: "PinIt 종료", "aria-label": "PinIt 종료", onClick: () => void ipc.quit() }, icon("power")),
      ),
    );
  }
}
