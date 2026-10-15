# PinIt 개선 플랜

> 작성: 2026-10-15 · 대상: `C:\Users\kiamm\Documents\개발\pinit` (Electron 33 + Vue 3 + Chrome MV3 확장)
> 상태: **분석 완료 / 사용자 승인 대기** (코드 변경 없음)

## Status (post-compact 시 여기부터)
- [x] 소스 전수 파악 (main.ts 1944줄, preload, Viewer.vue, Home.vue, extension, scripts)
- [x] 버그/구조 문제 목록화
- [x] 기술 선택 권고
- [x] 사용자 결정(2026-10-15): **A(Tauri 2)**, 별개 프로젝트 `C:\Users\kiamm\Documents\개발\pinit-next`, 구앱 핫픽스 **안 함**
- [x] 사용자 기능 변경 목록 수신 → §1.5에 반영 (§5/§6 중 충돌하는 부분은 §1.5가 우선)
- [x] §1.5 미결 질문 답변 수신·반영 완료
- [x] 환경 세팅: `pinit-next` 생성(Tauri 2 vanilla-ts), git init(main, 커밋 전), Cargo 워크스페이스(`crates/pinit-core` 순수 코어 + `src-tauri` 어댑터), `docs/DEPENDENCIES.md`, 코어 `geometry::Rect` + 테스트 2개 통과
- [x] 첫 Tauri 빌드 성공(`cargo build -p pinit-next`, 2m37s, MSVC 링크 OK)
- [x] 사용자 지시: 스캐폴드 커밋 완료, 이후 순서는 Claude 재량으로 진행, 질문 최소화
- [x] 코어 `rng`(SplitMix64), `placement`(FrontierPlacement: 앵커 8종 × Grid/Jigsaw, relayout best-of-seeds) + 테스트 13개 통과
- [x] 코어 완성(테스트 36개 통과): `view_state`, `image_ref`(key 정규화), `ports`, `resolver`(ChainResolver), `service`(OpenService: dedup→resolve→display→place→host, set_view refit, relayout_all, snapshot/restore). 파이프라인 Step 추상화는 YAGNI로 보류(명시적 서비스 + trait 주입/데코레이터)
- [x] 스파이크 1·2 완료 → `docs/SPIKES.md`. **중요**: 기본 설정 Tauri는 Electron 대비 메모리 이득 없음(20창: Tauri 1049MB private vs Electron 706MB). `--process-per-site`로 20창이 7프로세스/working set 410MB로 감소(Electron 1626MB) → 앱은 필수 적용. 지오메트리를 물리 px로 변경(`Display.scale`, DIP→px는 배치 시 디스플레이별 변환)
- [x] 어댑터/컴포지션 루트 완료(컴파일·단위테스트 통과): resolvers(File/Bytes/Url+디스크 캐시, Referer), store(settings/layout JSON 원자적 저장), displays, tauri_host(투명·프레임리스 창, asset 스코프 최소권한), cli(인자 파싱), app(AppState, 이벤트 펌프 스레드로 데드락 방지, 종료 시 레이아웃 보존), commands(IPC), 트레이, 단일 인스턴스, `--process-per-site`
- [x] 프런트엔드 완료·실동작 검증(CDP로 웹뷰 내용만 캡처): DESIGN.md + design.css 토큰, SVG 아이콘, 뷰어(줌/팬/90°·자유각 회전/반전, 창 비율 자동 맞춤, 리사이즈 그립), 설정창(배치 격자/자율, 앵커 선택, 디스플레이, 다크/라이트/시스템). 회전 시 창이 비율에 맞게 즉시 바뀌고 이미지 변환 순서 정확함 확인. 앱 실행 시 레이아웃 복원·저장 확인
- [x] 브라우저 연동 완료: 확장(우클릭 메뉴, 권한 contextMenus+nativeMessaging만) + Native Messaging 호스트(같은 exe, 열린 포트 없음, 레지스트리 HKCU 자동 등록). 가짜 브라우저로 end-to-end 검증(첫 메시지가 앱 기동, 이후 메시지는 실행 중 인스턴스로 전달, file:// 등 거부)
- [x] 포터블: `portable.flag` 옆 exe → 모든 데이터가 `PinIt-data/`(AppData 무접촉 확인), 확장은 exe에 내장, 단일 6.3MB exe. `npm run build:portable` → `release-portable/PinIt/`
- [x] 릴리스 실행에서 UI 스레드 데드락 발견·수정(락 잡는 IPC 커맨드는 전부 async). 20창 스트레스(동시 다중 인스턴스 실행) 응답 정상, 겹침 0, 다시 정렬 정상
- [x] 사용자 피드백 반영: 리사이즈 시 이미지 비율 강제 맞춤 제거(수동 "비율에 맞춤" 버튼만), 자유 회전 시 자동 배율 보정 제거, 이미지 뒤 배경 색/투명도 설정(기본 검정 40%, 열린 창에 즉시 반영), 90° 회전은 창 가로세로 맞바꿈
- [x] 사용자 피드백 2차 반영: "다시 정렬 시 현재 크기 유지" 토글(place_exact), 새 창을 띄울 화면 버그(모니터 id 64비트가 JS에서 손상 → 52비트로 마스킹), 혼합 DPI에서 창이 2/3로 줄던 버그(WM_DPICHANGED; apply_bounds가 위치→크기 순서+재시도), .pinit 레이아웃 저장/불러오기(세션 파일과 같은 형식), 개발 가이드(docs/DEVELOPING.md)와 격리된 `npm run dev:app`(식별자 com.pinit.next.dev, 레지스트리 무접촉)
- [ ] **다음**: 확인되지 않은 것: 실제 Chrome에서 확장 로드(수동), 인스톨러(NSIS)/파일 연결, 아이콘 교체, README + Native Messaging 호스트(`chrome-extension://` 인자 감지), 패키징(NSIS, 파일 연결)
- [ ] (보류) Phase 1 잔여 스파이크(Native Messaging 바이트 전달, 투명창 드래그·자유각도 회전)
- [ ] (원 항목)(투명창 20개 메모리, Native Messaging 바이트 전달, 혼합 DPI), DESIGN.md(/design-consultation), 어댑터, UI, 확장
- 메모: Bash heredoc에 긴 Rust 소스 넣으면 실패 → 파일은 Write 도구 사용
- [ ] Phase 0 ~ 7 (아래)

---

## 1. 현재 앱 요약

Pinterest 이미지를 always-on-top 프레임리스 창으로 띄우는 트레이 상주 유틸.
입력 4종(Chrome 중클릭/우클릭 확장, 클립보드 감시, 글로벌 단축키 Ctrl+Shift+P, 로컬 이미지 파일 연결) → 창 생성 → "퍼즐" 자동 배치 → 레이아웃 저장/복원/내보내기.

구조: `electron/main.ts` **1944줄 단일 파일**에 창 관리·배치 알고리즘·스크래핑·WS 서버·클립보드·저장소·다운로드·IPC 30여 개가 전부 전역 함수/전역 변수로 섞여 있음. 렌더러는 Vue 컴포넌트 2개(Home 938줄, Viewer 699줄).

## 1.5 기능 변경 명세 (사용자 요청, 버그 수정과 별개)

### 제거
- **Pinterest 전용 동작 전부**: Pinterest에서만 휠클릭을 가로채 PinIt으로 여는 content script 삭제. Pinterest도 일반 사이트 이미지와 동일 취급.
  - 연쇄 삭제: `content.js`, `*.pinterest.com` 호스트 권한, Pinterest 로그인 창/세션(`persist:pinterest`), 숨은 BrowserWindow 스크래퍼, `debugMode` 설정.
  - 도메인 모델 일반화: `PinRef` → **`ImageRef`**(이미지 URL / 페이지 URL / 로컬 파일 / 바이트). 앱 이름은 PinIt 유지 가능하나 코드에 "pinterest" 하드코딩 0.

### 개선 (우클릭 "PinIt으로 열기")
이제 브라우저 연동의 **유일한 경로**. 아이디어 후보(확정 아님, Phase 1 스파이크에서 가능성 확인):
1. **Referer 전달**: 핫링크 차단 사이트 대응 — 확장이 `pageUrl`을 같이 넘기고 앱이 Referer로 사용.
2. **확장이 바이트를 직접 fetch해 전달**: 사용자 쿠키·로그인 상태로 받은 이미지를 Native Messaging(확장→호스트 방향 대용량 허용)으로 전송 → 로그인 필요/핫링크/`blob:`·`data:` URL 모두 해결. `ImageResolver`의 한 구현체(`BytesFromBrowser`).
3. **원본 해상도 추정**: `srcset` 최대값, 감싸는 `<a href>`가 이미지면 그것 사용.
4. 링크 우클릭("링크의 이미지를 PinIt으로 열기"), 선택 영역 내 이미지 일괄 열기 — 후순위.

### 변경 — UI 전면 개편
- **메인(코어) 창 재설계**: 현재 `Home.vue`의 기본 폼 스타일 탈피. 이미지 창(Viewer)은 골격 유지하되 시각 정리.
- **이모지 전면 금지 → 벡터 아이콘**: 인라인 SVG 스프라이트(자체 제작 또는 ISC/MIT 아이콘셋에서 필요한 것만 복사, 런타임 의존 0). 아이콘 한 세트로 통일(stroke 두께·모서리 규격 고정).
- 디자인 방향은 구현 전에 `/design-consultation`(DESIGN.md 생성) 또는 `/design-shotgun`(시안 비교)으로 확정. 토큰(색/타이포/간격/모션)을 DESIGN.md → CSS 변수 단일 소스로.

### 변경 — 자동 배치 2모드 (구앱은 계산 버그로 단일 "퍼즐"에서 멈춘 미완성 기능)
1. **격자(Grid) 모드**: 크기 프리셋(예: S/M/L/사용자 정의)을 고르면 그 크기로 격자에 순서대로 채움.
2. **자율(Jigsaw) 모드**: 이미지마다 약간 랜덤한 크기·종횡비로, **남은 공간에 빈틈 없이 딱 맞게** 퍼즐 조각처럼 계속 채움.
- 구현 시 **원칙: 모든 지오메트리를 DIP 정수 사각형 + 순수 함수로**, 모니터 workArea 기준. 구앱 버그의 원인이 해상도/DPI/창 크기 혼용이었으므로 코어에서 DPI를 아예 모르게 하고 어댑터 경계에서만 변환. 속성 기반 테스트(겹침 0, 영역 밖 0, 커버리지 비율, 시드 재현성).
- 아키텍처 영향 (§5 보강):
  - `PlacementStrategy`는 이제 **배치 + 크기 결정**을 함께 담당: `place(PlacementContext{existing, workArea, imageAspect, preset, rng}) -> Rect`. 구현: `GridPlacement`, `JigsawPlacement`(free-rectangle 분할/maximal-rectangles 계열). 설정으로 교체.
  - **파이프라인 순서 변경**: 이미지 종횡비가 필요하므로 `Resolve`(크기 확인 포함) → `Place` → `Spawn`. 창은 해석 완료 후 최종 위치에 바로 뜸(구앱의 "로딩 창 먼저 띄우고 나중에 크기 바꿈" 점프 제거). 로딩 지연이 길면 임시 플레이스홀더 슬롯 예약 Step을 추가.
  - `Relayout` 명령: 창이 닫혀 생긴 빈 공간을 재배치로 메우는 "다시 정렬" 동작(자동/수동은 미결 질문).
  - 시드 RNG를 주입(`Rng` 포트) → 테스트·"같은 배치 다시 보기" 가능.

### 배치 핵심 원칙: 프런티어 채우기 (사용자 아이디어, 확정)
- 배치는 **시작 앵커(모서리 4종 또는 변 4종, 설정 가능)에서 시작해 반대쪽으로 채워 나간다.** 점진 배치로 어쩔 수 없이 생기는 틈은 이미 채운 영역 안쪽이 아니라 **채우는 전선(frontier) 쪽으로 몰아** 사용자에게는 "아직 안 채운 부분"으로 보이게 한다.
- 구현 방향: skyline / shelf 계열 프런티어 관리 (`FrontierPlacement`). Grid·Jigsaw 둘 다 같은 프런티어 위에서 동작하고 *크기 결정 규칙*만 다름(Grid=프리셋 행 높이, Jigsaw=랜덤 배율). 틈이 프런티어 안쪽에 생기면(이미지 비율 때문에 불가피) 다음 배치가 그 틈부터 메우도록 후보 점수에 "프런티어 평탄도" 가중.
- 테스트: 앵커 8종 × 모드 2종 × 시드 N개 속성 테스트 — 겹침 0, 영역 밖 0, *채운 영역 내부 틈 면적 비율 상한*.
- "다시 정렬"도 같은 앵커·프런티어 규칙으로 전체 재패킹.

### 추가 — 이미지 회전 / 좌우반전
- 창별 `ViewState = { zoom, pan, rotation, flipH, flipV }` 값 객체로 확장, 레이아웃 저장/복원/.pinit 내보내기에 포함(스키마 버전 올림).
- `창 비율에 맞춤`과 배치 계산은 회전 후 유효 종횡비(90°/270° 시 가로세로 교환)를 사용.
- 변환은 렌더러에서 CSS transform 조합(줌·팬과 순서 고정), 로직은 순수 TS 클래스로 분리해 테스트. 툴바 버튼 + 단축키(예: `R`/`Shift+R` 회전, `F` 반전).

### 확정된 결정 (2026-10-15 사용자 답변)
| 항목 | 결정 | 설계 반영 |
|---|---|---|
| 클립보드 자동 열기 / 글로벌 단축키 | 사용자가 기능을 잊을 만큼 안 씀 → **v1에서 제외** (사용자 확인: 둘 다 제거) | `InputSource` 구현체만 안 만듦. 포트 구조라 나중에 추가 비용 낮음. `global-shortcut` 플러그인·클립보드 폴링·og:image 범용 추출(OgImage resolver)도 v1 범위 밖. 입력은 **우클릭 메뉴 + 로컬 파일 열기(+ 설정 창 URL 입력)** |
| 격자 셀 | **이미지 비율 유지** — 프리셋이 한 변(행 높이)을 고정, 다른 변은 이미지 비율대로 가변 | `GridPlacement` = 행(shelf) 기반 흐름. 프리셋은 행 높이: **S 200 / M 300 / L 440 DIP** (임시 기본값, 사용감 보고 조정). 시작 코너·진행 방향은 설정 |
| 자율 모드 | **원본 비율 유지, 창이 이미지로 꽉 참(레터박스 없음)** | 창 종횡비 = 이미지 종횡비(회전 반영). 점진 배치는 free-rectangle(guillotine) 분할로 남은 공간 중 *들어가는 가장 좋은 자리*에 랜덤 배율(기준 0.7~1.4배, 설정 가능)로 배치. 비율 고정이라 수학적 완전 타일링은 불가 → 점진 배치는 잔여 틈 허용, **"다시 정렬"에서 전체를 재패킹해 틈 최소화**(행 균등 높이/이진 분할 트리 탐색 후 영역에 맞춰 스케일) |
| 빈자리 | **수동 "다시 정렬" 버튼** (자동 재정렬 없음) | `Relayout` 명령 1개, 메인 창·트레이에서 호출. 현재 모드(Grid/Jigsaw)로 전체 재배치 |
| 회전 | **자유 각도 필요** (+ 90° 단위 빠른 회전, 좌우/상하 반전) | `rotation`은 도(°) 실수. 창은 투명하므로 자유 각도는 *창 크기 고정, 이미지가 창 안에서 회전*(모서리 투명) + "맞춤" 옵션. 90° 단계 회전은 창 종횡비 교환(재맞춤). 각도 입력: 단축키 단계 / 슬라이더 / 휠+수정키 |
| 디자인 | **다크/라이트 둘 다** (시스템 따라가기 + 수동 선택). 레퍼런스는 위임 | `/design-consultation`에서 레퍼런스 조사 후 DESIGN.md 확정. 색 토큰 라이트/다크 2세트를 CSS 변수로 |

---

## 2. 발견한 버그 (코드 읽고 확인한 것)

심각도: 🔴 데이터 손실/보안/기능 불능 · 🟡 오동작 · 🟢 품질

| # | 심각도 | 위치 | 내용 |
|---|---|---|---|
| 1 | 🔴 | `main.ts:879,966-971` | 스크래핑으로 찾은 실제 이미지 URL이 **렌더러로만 전송되고 `WindowInfo`/store에는 저장 안 됨**. 클립보드·단축키로 연 핀은 `imageUrl`이 핀 *페이지* URL → "모든 이미지 다운로드"(`:1523`)와 .pinit 내보내기가 **HTML을 .jpg로 저장**. 복원 때마다 재스크래핑 |
| 2 | 🔴 | `main.ts:1319` | WS 서버가 `{port}`만 지정 → **모든 인터페이스에 바인딩, 인증/Origin 검사 없음**. LAN 기기나 아무 웹페이지(`new WebSocket('ws://localhost:19191')`)가 창을 열게 할 수 있음 |
| 3 | 🔴 | `main.ts:960` | `/pinterest\.com/` 정규식이 앵커 없음 → `http://evil.com/pinterest.com`도 통과해 **로그인 세션(`persist:pinterest`)이 붙은 창에서 임의 페이지 로드 + `executeJavaScript`** (#2와 연결되면 원격 트리거 가능) |
| 4 | 🔴 | `main.ts:119,174,932` | `isAppQuitting`이 트레이 "종료" 클릭에서만 true. OS 종료·`app.quit()`·업데이트 시 메인 창 close가 막히고, 이미지 창 close 핸들러가 **저장 목록에서 창을 삭제** → 레이아웃 소실 |
| 5 | 🔴 | `background.js:2,61-75` | MV3 service worker는 ~30초 idle 후 종료되는데 WS 객체·`setTimeout` 재연결에 의존 → 한동안 안 쓰다 중클릭하면 "Electron 앱이 실행되지 않았습니다"로 **첫 클릭 실패** |
| 6 | 🟡 | `main.ts:462-477` | 같은 URL 스크래핑 중 두 번째 요청은 완료 후 무조건 `null` resolve → 에러 화면 |
| 7 | 🟡 | `main.ts:11-12` | `force-device-scale-factor=1`로 DPI를 전역 강제 → 4K에서 흐림/작은 UI. 근본 원인(DIP/물리픽셀 혼동)을 덮은 임시방편. 확장은 `screenX`(CSS px, `content.js:121`)를 보내는데 main은 DIP로 해석 |
| 8 | 🟡 | `background.js:46-47` | 우클릭 메뉴 경로는 마우스 좌표를 `100,100` 고정 → 항상 같은 모니터 |
| 9 | 🟡 | `main.ts:1313`, `:1392-1413` | `nativeMessagingEnabled` 토글은 **시작 시 1회만 반영**. 런타임 토글하면 WS 서버 on/off 안 됨. 이름도 사실과 다름(native messaging 아님) |
| 10 | 🟡 | `main.ts:897-923,1076` | `move`/`resize` 이벤트마다 `electron-store` **동기 디스크 쓰기** (드래그 중 초당 수십 회) |
| 11 | 🟡 | `Viewer.vue:204-214`, `main.ts:1864` | 창 드래그를 mousemove마다 IPC로 delta 이동 → 지터/드ift, IPC 폭주. `-webkit-app-region: drag` 또는 main 측 커서 폴링으로 대체해야 함 |
| 12 | 🟡 | `main.ts:1019,1160` | 클립보드 텍스트 **전체**를 URL로 취급(`includes('pinterest.com')`). 문장 속 링크·추적 파라미터 달린 URL → 로드 실패 또는 중복 창(`:729` 문자열 완전 일치 비교) |
| 13 | 🟡 | `main.ts:1185,1243-1248` | `local-file://C:/...` 는 비표준 스킴에서 `C:`가 host로 파싱돼 드라이브 문자 유실 가능(미실행 검증, 확인 필요). 또 어떤 경로든 읽어줌(확장자 제한 없음), `registerSchemesAsPrivileged` 미호출 |
| 14 | 🟡 | `main.ts:1589-1623` | 다운로더: 303/307/308 리다이렉트 미처리, 횟수 제한·타임아웃 없음. 렌더러가 준 임의 URL을 main이 fetch(SSRF 표면) |
| 15 | 🟡 | `main.ts:334-340` | 후보 정렬 comparator가 임계값(30000) 기반이라 **비추이적** → 정렬 결과 비결정적 |
| 16 | 🟡 | `nsis-context-menu.nsh:3` | `HKCR\*\shell\PinIt` → **모든 파일 종류** 우클릭에 메뉴 추가. `perMachine:true` + `deleteAppDataOnUninstall:true`(레이아웃 삭제) |
| 17 | 🟢 | `Home.vue:159,235` | `downloadProgress` 갱신하는 곳 없음(죽은 UI). `onNotification`도 main이 보내는 곳 없음 |
| 18 | 🟢 | `Viewer.vue` 전반 | `(api as any)` 캐스팅 20여 곳(타입 정의 `electron.d.ts`가 preload와 불일치), IPC 리스너 해제 안 됨, 줌 저장 이중 호출(watch + zoomIn), `alert()` UX, `?image=` 파라미터는 main이 안 보냄 |
| 19 | 🟢 | `tsconfig.json` | `include`가 `src`뿐 + `strict:false` → `electron/main.ts`는 `vue-tsc`로 **타입 검사 안 됨** |
| 20 | 🟢 | 빌드 | `dist-electron/`에 해시 번들 **140개·135MB** 누적, `files: dist-electron/**/*` 때문에 **인스톨러에 전부 포함**. `canvas`(네이티브 DLL 40개)는 아이콘 스크립트 하나 때문에 devDep, `archiver`는 확장 zip 하나 때문에 |
| 21 | 🟢 | 잔재물 | `native-messaging-host*.json`·`install-native-messaging.ps1`은 죽은 코드(하드코딩 경로 `C:\Users\kiamm\OneDrive\...`), `extension/icon*.png` 0바이트, README는 없는 `router.ts`·`type-check` 스크립트 언급, git 저장소 아님 |
| 22 | 🟢 | `manifest.json` | `host_permissions: <all_urls>` + 미사용 `activeTab`. 필요한 건 pinterest + localhost뿐 |

## 3. 구조 진단 (OOP / 컴포지션 관점)

- **God file**: 책임 10여 개가 `main.ts` 하나에. 전역 가변 상태(`imageWindows`, `mainWindow`, `scrapingUrls`, `wss`, `tray`...) 직접 공유.
- **OCP 위반**: 입력 소스 추가(예: Firefox 확장, 드래그&드롭) = `createImageWindow` 시그니처·IPC·main 여러 곳 수정. URL 판별(`pinterest.com`)이 5곳에 복붙.
- **DIP 위반**: 로직이 `electron`, `electron-store`, `ws`, `https`를 직접 호출 → 단위 테스트 불가(배치 알고리즘조차 `WindowInfo`+`Electron.Display` 의존).
- **중복**: `SavedWindow`/`WindowInfo`/export 스키마/`Settings`가 main·d.ts·Home.vue에 3중 정의.
- 순수 로직(배치, URL 정규화, 크기 clamp)이 부수효과와 뒤섞여 있어 버그 #12·#15 같은 걸 테스트로 못 잡음.

## 4. 기술 선택: Electron은 deprecated 아님, 그러나 이 앱엔 과함

Electron 자체는 현역(활발히 유지). 문제는 **적합성**:
- 이미지 창 1개 = Chromium 렌더러 프로세스 1개 (창 20개 ≈ 1GB+ 가능), 설치 100MB+.
- DPI 해킹, 스크래퍼용 숨은 BrowserWindow, 로그인 창 — 전부 "Pinterest를 웹으로 긁는" 설계에서 온 부담.
- 앱의 본질(작은 투명 창 + 트레이 + 단축키 + 클립보드)은 OS 웹뷰로 충분.

| 옵션 | 장점 | 단점 |
|---|---|---|
| **A. Tauri 2 (Rust 코어 + 웹뷰 UI)** ⭐권장 | 설치 ~5-10MB, 창당 메모리 훨씬 작음(WebView2 공유), trait+struct 조합이 컴포지션 철학과 1:1, 단일 인스턴스/트레이/글로벌 단축키/파일 연결 공식 지원, 이미 `cargo 1.95` 설치됨 | Rust 학습 비용, WebView2 의존(Win11 기본 탑재), 플러그인 의존 몇 개 |
| B. Electron 유지 + TS 전면 재작성 | 언어 그대로, 가장 빠른 이행 | 메모리/용량 문제 그대로, 의존성 최소화 목표와 충돌 |
| C. C#/WPF | 네이티브 Win32 제어(투명/Z-order) 최고 | .NET SDK 미설치, Windows 전용, 웹 UI 자산 폐기 |

**권고: A (별개 프로젝트 `pinit-next`로 분리, 현 프로젝트는 레퍼런스/롤백용 보존).**
Rust가 부담이면 B로 가되 아래 아키텍처(§5)는 동일하게 적용 — 설계가 핵심이고 런타임은 교체 가능해야 함(=포트/어댑터의 이점).

## 5. 목표 아키텍처 — 헥사고날(포트&어댑터) + 컴포지션 루트

**원칙**: 상속 금지, 작은 trait(포트) + 구조체 조합. 모든 의존은 생성자 주입, `main`(컴포지션 루트) 한 곳에서만 구체 타입을 안다. 새 기능 = 새 구현체 추가 + 루트에 한 줄 등록 (기존 코드 수정 0).

```
 [입력 어댑터]  ──OpenRequest──►  [파이프라인(미들웨어 체인)]  ──►  [WindowHost 포트]
 Clipboard                         Normalize → Dedup → Resolve        Tauri 창 어댑터
 Hotkey                            → Place → Spawn → Persist
 BrowserBridge(확장)                     │        │       │
 FileOpen / DragDrop                     ▼        ▼       ▼
                               PinRef  ImageResolver  PlacementStrategy   (+ Store, Downloader 포트)
```

### 코어(도메인, 외부 의존 0, 100% 단위 테스트 대상)
| 개념 | 형태 | 비고 |
|---|---|---|
| `PinRef` | 값 객체 | URL → 정규화된 핀 ID(`/pin/{id}`) 추출. 추적 파라미터 제거 → 중복 판정 안정(#12) |
| `OpenRequest` | 값 객체 | `{source, pin|file, imageHint?, cursor?, displayHint?}` — 출처와 무관한 단일 입력 형식 |
| `WindowLayout` | 값 객체 | 위치/크기/줌/팬. **DIP 단위로 통일**, 스키마 버전 필드 |
| `InputSource` (trait) | 포트 | `start(sink)`/`stop()` — 구현: Clipboard, Hotkey, BrowserBridge, FileOpen |
| `ImageResolver` (trait) | 포트 | `resolve(pin) -> Option<ImageUrl>` — 구현: Direct, ProvidedByBrowser, PinterestHtml(og:image), (옵션) HiddenWebview. **`ChainResolver`가 순서대로 시도** — 데코레이터로 `CachingResolver`, `RetryResolver` 감쌈 (#1·#6) |
| `PlacementStrategy` (trait) | 포트 | `place(existing, workArea, size) -> Rect` — `PuzzlePlacement`(현 알고리즘, 결정적 정렬), `CascadePlacement`; 설정으로 교체 |
| `Pipeline` | `Vec<Box<dyn Step>>` | 각 Step이 `OpenRequest`를 변환/중단. 로깅·레이트리밋·중복차단도 Step |
| `LayoutStore` / `SettingsStore` (trait) | 포트 | 구현: `JsonFileStore`. 쓰기는 `DebouncedStore` 데코레이터(#10) |
| `Downloader` (trait) | 포트 | 리다이렉트/타임아웃/크기 제한/호스트 allowlist 내장(#14) |
| `WindowHost` (trait) | 포트 | `open/close/set_bounds/on_moved…` — 코어는 Tauri를 모름 → 테스트용 `FakeHost` |
| `OriginUpgrader` | 정책 객체 | `236x/474x/…` → `originals` 변환을 정규식 나열 대신 규칙 테이블화 |

### 어댑터 / 인프라
- **브라우저 연동**: 개방 포트 제거가 최선 → **Chrome Native Messaging** (같은 exe를 `--native-host`로 실행, stdio, `allowed_origins`로 확장 ID 고정, HKCU 레지스트리 1개). 대안(스파이크 결과에 따라): `127.0.0.1` 바인딩 + 확장↔앱 공유 토큰 + Origin 검사. 어느 쪽이든 `BrowserBridge`는 `InputSource` 하나일 뿐.
- **[§1.5로 대체됨] Pinterest 특화 제거**: 아래 서술은 원안. 최종은 Pinterest 전용 경로 없이 범용 `ImageResolver` 체인(Direct → BytesFromBrowser → OgImage).
- (원안) **Pinterest 해석 전략 전환**: 숨은 창 스크래핑이 불안정/무거운 근원. 확장은 *이미 로그인된 사용자 브라우저의 DOM*에서 실제 이미지 URL을 알 수 있으므로 **확장이 해석해서 URL을 넘기는 것을 1순위**, 클립보드/단축키(확장 없음)는 `PinterestHtml`(HTTP로 `og:image`) 2순위, 필요 시 HiddenWebview 3순위. → 로그인 창/세션 관리 코드 삭제.
- **DPI**: `force-device-scale-factor` 제거. 좌표는 전 구간 DIP(논리) 유지, 확장은 모니터 힌트를 보내지 말고 앱이 `cursor_position()`로 결정(#7,#8).
- **창 이동**: `data-tauri-drag-region`(OS 네이티브 드래그) → IPC 폭주 제거(#11). 줌/팬은 UI 내부 상태.
- **로컬 파일**: Tauri `asset` 프로토콜 + 스코프 제한(이미지 확장자만)(#13).
- **UI**: 프레임워크 제거 — 순수 TS + 작은 클래스(`ViewerView`, `ZoomPanController`, `Toolbar`) 조합, 페이지 2개(HTML 2장). 번들러는 Vite 하나 또는 `tsc`만. 라우터·Vue 제거 → 프론트 런타임 의존 0.
- **IPC 계약**: Rust 타입에서 TS 타입 자동 생성(`ts-rs` 등) 또는 단일 `contracts` 정의 → `any` 캐스팅 제거(#18).
- **설정**: `Settings`는 serde 구조체 하나(단일 진실원), 변경 시 `SettingsChanged` 이벤트 → 구독자(Clipboard 소스, 핫키, 브리지)가 스스로 start/stop (#9). 설정 토글이 런타임에 즉시 반영.

### 의존성 예산 (목표)
- Rust: `tauri`(+ plugins: single-instance, global-shortcut, dialog), `serde`/`serde_json`, HTTP 클라이언트 1개(`ureq` 등), 로깅(`tracing` or `log`). 그 외 추가는 PR마다 사유 기록.
- JS: `typescript` + 번들러 1개 (dev only). 런타임 의존 0.
- 제거: vue, vue-router, electron, electron-store, ws, archiver(확장 zip은 번들 리소스 그대로 복사), canvas, concurrently, wait-on, vite-plugin-electron*.

## 6. 단계별 실행 계획

**Phase 0 — 동결 & 안전망** (0.5일)
- [ ] 현 프로젝트 `git init` + 첫 커밋(소스만; `dist-electron`·`release` 제외 확인)
- [ ] 사용자 데이터 백업: `%APPDATA%\pinit\config.json` (savedWindows·settings) — 마이그레이션 샘플로 사용
- [ ] (선택, 구앱 계속 쓸 경우) 핫픽스 3종: WS `host:'127.0.0.1'`+Origin 검사 / 스크래핑 결과를 `WindowInfo.imageUrl`+store에 저장 / `before-quit`에서 `isAppQuitting=true`. 새 앱 완성 전 임시 방편, 30분 내외.

**Phase 1 — 스파이크 (리스크 제거)** (1일) ← 본 작업 전 반드시
- [ ] `og:image`가 HTTP 응답 HTML에 실제로 있는지, 봇 차단/로그인 벽 여부 (핀 10개 샘플, 공개/비공개)
- [ ] Tauri 투명·프레임리스·always-on-top 창 20개 메모리/기동 시간 측정 vs 현 Electron
- [ ] Native Messaging(같은 exe 재실행 → 기존 인스턴스로 전달) 동작 PoC
- [ ] 4K + 혼합 DPI 멀티모니터에서 DIP 좌표 일관성
- 결과에 따라 §4 A/B 확정, §5 브라우저 연동 방식 확정

**Phase 2 — 코어** (2-3일)
- [ ] `PinRef`, `OpenRequest`, `WindowLayout`, 포트 trait 정의
- [ ] `PuzzlePlacement` 이식(결정적 tie-break) + 테스트: 빈 화면/가득 찬 화면/멀티모니터/작업표시줄
- [ ] `Pipeline` + Step(Normalize, Dedup, Resolve, Place, Spawn, Persist) + `FakeHost` 통합 테스트
- [ ] `ChainResolver` + 캐시/재시도 데코레이터 (가짜 HTTP로 테스트)

**Phase 3 — 어댑터** (2-3일)
- [ ] JsonFileStore(+Debounced, 스키마 버전, 구 config/.pinit v1.1 마이그레이션)
- [ ] Clipboard / Hotkey / FileOpen / BrowserBridge InputSource
- [ ] Downloader(allowlist·리다이렉트·타임아웃), 일괄 다운로드 + 진행률 이벤트
- [ ] 트레이, 단일 인스턴스, 종료 시 레이아웃 보존(#4)

**Phase 4 — UI** (2일)
- [ ] Viewer(줌/팬/툴바/오류·로딩 상태), 네이티브 드래그
- [ ] Settings(설정 변경 → 즉시 반영), 레이아웃 내보내기/불러오기

**Phase 5 — 확장 재작성** (0.5-1일)
- [ ] MV3: content script 삭제, 권한 최소화(`contextMenus`, `nativeMessaging` + 필요 시 `scripting`/host), service worker 생존 문제 제거(요청 시점 연결), 우클릭 메뉴만 지원(§1.5 개선 아이디어 반영), 좌표 문제 제거
- [ ] Phase 4/5 추가: 회전·반전(ViewState), 배치 2모드(Grid/Jigsaw) 및 설정 UI, 벡터 아이콘 세트, 디자인 시스템(DESIGN.md) 선행

**Phase 6 — 패키징** (1일)
- [ ] 사용자 단위 설치(perMachine 제거), 파일 연결은 이미지 확장자 한정(`SystemFileAssociations`), 언인스톨 시 데이터 삭제 여부 선택
- [ ] 아이콘 생성 스크립트 → 정적 자산으로 교체(canvas 제거), README/가이드 전면 갱신

**Phase 7 — 동등성 QA & 전환**
- [ ] 기능 체크리스트: 중클릭·우클릭·클립보드·단축키·파일열기·퍼즐 배치·복원·내보내기/불러오기·일괄 다운로드·줌/팬 영속·모니터 선택·창 비율 맞춤·트레이
- [ ] 위 버그 #1~#22 각각 회귀 테스트 또는 수동 시나리오 통과 확인
- [ ] 구앱 아카이브, 데이터 마이그레이션 1회 실행

## 7. 리스크 / 미확인 사항
- Pinterest의 `og:image` HTTP 접근 가능성은 **미검증** 가정 — Phase 1 스파이크 결과가 해석 전략 전체를 좌우. 불가하면 확장 해석 + (옵션) 숨은 웹뷰 전략을 `ImageResolver` 구현체로 추가(구조상 비용 낮음).
- 버그 #13(`local-file://` 드라이브 문자)은 코드 정독 기반 추정, 실행 확인 안 함.
- Pinterest DOM/약관 변경에 취약한 본질적 한계는 어떤 런타임을 써도 동일.
- Tauri/WebView2에서 투명 창·Z-order 동작은 Electron과 미세하게 다를 수 있음(스파이크 항목).

## 8. 결정이 필요한 것
1. 방향: **A(Tauri, 별개 프로젝트 `pinit-next`)** vs B(Electron TS 재작성) — 권고 A
2. 구앱 임시 핫픽스(Phase 0 선택 항목) 할지
3. 새 프로젝트 위치/이름 (기본: `C:\Users\kiamm\Documents\개발\pinit-next`)
