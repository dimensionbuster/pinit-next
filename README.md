# PinIt

항상 위에 떠 있는 이미지 창 (Windows, Tauri). 브라우저 확장으로 웹 이미지를 우클릭해 바로 핀 고정할 수 있습니다.

## 설치

[Releases](https://github.com/dimensionbuster/pinit-next/releases)에서 `PinIt-vX.Y.Z-windows-portable.zip`을 받아 압축을 풀고 `PinIt.exe`를 실행합니다. 설치가 필요 없습니다.
Windows 10은 Microsoft Edge WebView2 Runtime이 필요합니다.

새 버전이 공개되면 설정 창 상단에 알림 배너가 뜹니다. (알림만 — 자동 교체는 하지 않으므로 릴리스 페이지에서 직접 받으세요.)

## 개발

```
npm ci
npm run dev:app        # 개발 실행
npm run check          # tsc + 테스트
npm run build:portable # release-portable/PinIt 생성
```

자세한 내용은 [docs/DEVELOPING.md](docs/DEVELOPING.md).

## 릴리스 절차

1. `node scripts/version.mjs x.y.z` — package.json, Cargo.toml, tauri.conf.json, Cargo.lock 버전을 한 번에 갱신
2. 커밋 후 PR → `main` 머지
3. `Release` 워크플로가 `vx.y.z` **초안(draft)** 릴리스와 포터블 zip을 자동 생성 (같은 버전의 릴리스가 이미 있으면 건너뜀)
4. 초안의 노트를 다듬고 **Publish** — 공개되는 순간부터 실행 중인 앱이 업데이트 알림을 표시
