# rust-stock

<p align="center">
  <img src="logo.png" alt="rust-stock logo" width="150" />
</p>

<p align="center">
  <b>헤지펀드의 투자위원회 전체를, 화면 구석 스마트폰 크기의 플로팅 창에 담다.</b><br/>
  A주 실시간 시세 · 5대 유파 AI 합의 엔진 · 공급망 병목 지점 특정 · 완전 로컬 실행 · 데스크톱 + Android
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="License: Apache-2.0"/></a>
  <img src="https://img.shields.io/badge/Tauri-2.x-24C8DB.svg" alt="Tauri 2"/>
  <img src="https://img.shields.io/badge/Rust-2021-orange.svg" alt="Rust"/>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Android-success.svg" alt="Platform"/>
  <img src="https://img.shields.io/badge/AI-DeepSeek%20%7C%20OpenAI--compatible-7C3AED.svg" alt="AI"/>
</p>

<p align="center">
  <a href="README.md">简体中文</a> · <a href="README.en.md">English</a> · <a href="README.ja.md">日本語</a> · <b>한국어</b>
</p>

---

## ⚡ 무엇인가

다른 도구는 AI **하나**의 의견을 준다. rust-stock 은 **가치 · 성장 · 단기자금 · 기술적 · 매크로** 다섯 유파의 AI 애널리스트를 한 무대에 올려 토론시키고, 헤지펀드 투자위원회처럼 **종합 판정**한다. 그리고 그것은 데스크톱이나 휴대폰 구석의 360×640 프로스트 플로팅 창——화면 가장자리로 끌면 자동으로 흡착되어 접히는——일 뿐이다. 빨강이 상승·초록이 하락(A주 관례), **완전 로컬 실행·서버 불필요**, 데이터는 본인 기기를 벗어나지 않는다.

데스크톱(Windows / macOS)과 Android 는 **동일 소스·동일 경험**.

> ⚠️ 모든 AI 출력은 **리서치 순위와 아이디어 참고일 뿐, 투자 자문이 아닙니다**. 시장에는 위험이 있으며, 결정은 본인 책임입니다.

## 🧠 의사결정 엔진: "AI에게 오를지 묻기"가 아니라 하나의 방법론

"오늘의 AI 추천"의 핵심은 **실데이터 → 멀티 에이전트 판정 → 시세 재확인** 파이프라인으로, 두 가지 우수한 공개 방법론을 융합한다.

**① 로컬 전체 시장 스캔(실데이터, AI가 지어내지 않음)**
매일 먼저 본인 기기에서 실시세로 후보 풀을 선별: **상승률 상위 + 주력자금 순유입 상위 + 룽후방(대량거래 순위) 등재**를 병합·중복 제거하고, 현재가/등락률/회전율/주력 순액/등재 여부를 부여한다. AI에게는 확고한 실수치만 전달해 "환각 시세"를 원천 차단한다.

**② 공급망 병목 연구법(토대) — [muxuuu/serenity-skill](https://github.com/muxuuu/serenity-skill) 참고**
시장 내러티브를 **체계적인 물리적 제약**으로 번역한다: 산업 체인을 8개 층으로 분해(전방 수요 → 시스템 통합 → 모듈 → 칩 → 공정·패키징 → 장비·검사 → 소재·소모품 → 인프라)하고, 가장 **병목이 되는 희소 층**(공급사 집중도 / 인증 주기 / 증설 난도 / 공정 장벽)을 찾아낸 뒤, 그 층을 **지배**하는 자가 누구인지 특정한다.
> **채택 이유**: 테마는 부풀려지고 이야기는 바뀌지만, 생산능력·수율·인증 같은 물리적 병목은 거짓말하지 않는다. 공급망의 실제 제약에 닻을 내려야 감정을 쫓는 대신 가치가 실제로 축적되는 지점을 찾을 수 있다.

**③ 다유파 합의 채점(판정) — [virattt/ai-hedge-fund](https://github.com/virattt/ai-hedge-fund) 참고**
각 후보에 대해 다섯 투자 유파의 AI 관점이 **각자 독립적으로 채점한 뒤 중재**한다:

| 유파 | 무엇을 보는가 |
|---|---|
| 💎 가치 | 해자(진입장벽), 밸류에이션 안전마진 |
| 🚀 성장 | 산업 S곡선, TAM 천장 |
| 🔥 단기자금 | 테마 열기, 룽후방, 회전·거래량 에너지 |
| 📈 기술적 | 추세, 돌파, 이동평균 구조 |
| 🌐 매크로 | 정책, 자금면, 유동성 |

> **채택 이유**: ai-hedge-fund 의 정수는 "다수의 투자자 에이전트 + 중재"다. 단일 관점에는 반드시 사각지대가 있다. 여러 전문 유파가 먼저 토론해 이견을 드러내고 위험을 노출시키는 편이 "오를까?"라고 묻는 것보다 훨씬 신뢰할 만하다.

**④ 종목별 심층 리서치 + 실시세 재확인**
2단계 출력: 먼저 6~8 종목을 1차 선별하고, 다음으로 **선정된 각 종목에 대해 개별적으로 공급망 8층 심층 분석**("연구" 버튼과 동일 소스: 체인 내 위치 / 병목과 희소성 / 5유파 강세약세 / 촉매와 검증 / 위험 / 반증 / 리서치 우선순위)을 수행해 추천 사유에 통합한다(병렬 실행으로 소요 시간 제어). 이후 모든 가격/등락률은 실시세로 **되채움**(AI의 수치 날조 금지), 환각 코드와 **6거래일 연속 하락(6연속 음봉)** 종목은 제외. 각 종목에 첨부: **체인 내 위치 / 5유파 이견 / 룽후방 자금 신호 / 오늘의 관전 포인트 / 주요 위험 / 반증 조건**.

> 한마디로: **실데이터를 토대로, 공급망 병목으로 방향을 정하고, 5유파로 취사를 결정하며, 실시세로 보증한다.** 상한가를 부추기지 않고, 종목을 외치지 않으며, 근거 있는 리서치 순위만 매긴다.

## 📊 또 무엇이 있나

- **시장 심리 게이지**: 4대 지수(상하이종합/선전성분/창업판/CSI300)의 등락률 가중 + tanh 압축을 -100~100 바늘에 실시간 반영; 클릭하면 3D로 뒤집혀 계산 명세 + AI 장세 해설.
- **관심종목 AI 건강검진**: 각 종목 옆에 미니 게이지, AI가 -100~+100으로 강세약세 채점; 탭하면 공급망식 상세 사유.
- **추천 썸네일**: 각 추천 종목 뒤에 실데이터 "최근 30일 종가 라인", 한 번 탭으로 일봉 차트로.
- **차트 / 자금 흐름 / 칩 분포 / 기술적 지표**: 일/주/월봉 캔들 + MA5/MA10 + 거래량; 통신(TDX) 기준 MACD/KDJ/RSI/BOLL 현재값과 골든/데드 크로스; 개별 종목 자금 흐름 5단계(주력/초대형/대형/중형/소형, 빨강=유입·초록=유출); **칩 분포도**——회전율 감쇠 + 원가 중추 가우시안 커널로 가격대별 칩 누적을 근사(빨강=이익 물량/초록=손실 물량), 이익 비율/평균 원가/현재가를 직접 표시.
- **AI 스트리밍 채팅 + 심층 리서치 + 즐겨찾기**: 하단 바가 DeepSeek에서 한 글자씩 스트리밍; "연구" 버튼으로 8층 공급망 심층 워크플로 진입; 결론은 **그룹으로 저장** 가능, 전체 또는 항목별 복사, 기기에 영구 저장.
- **장외 시간 안내**: A주가 개장 전/점심 휴장/마감/주말일 때 시세 바 아래 빨간 띠가 "직전 거래일 데이터 사용 중"을 명시해, 오래된 데이터를 실시간으로 오인하지 않게 한다.
- **성과 백테스트**: 과거 추천을 차트로 재계산해 승률과 거래당 수익을 산출, 엔진이 스스로를 감독.

## 🖼️ 스크린샷

> 데스크톱 + Android 동일 소스; 프로스트 듀얼 테마(낮=크림 화이트/밤=순흑). 스크린샷은 계속 업데이트.

| 시세 홈 | 관심종목 + AI 게이지 | 오늘의 AI 추천 |
|:---:|:---:|:---:|
| ![Market](docs/screenshots/market.png) | ![Watchlist](docs/screenshots/watchlist.png) | ![Picks](docs/screenshots/recommend.png) |

| 차트 + 자금 흐름 | 리서치 즐겨찾기 | 설정 |
|:---:|:---:|:---:|
| ![K-line](docs/screenshots/kline.png) | ![Research](docs/screenshots/research.png) | ![Settings](docs/screenshots/settings.png) |

## ✨ 기능 목록

- **플로팅 창 경험(데스크톱)**: 테두리 없는 둥근 모서리 최상위 미니 창, 타이틀 바로 드래그, 화면 가장자리에서 자동 흡착; ✕로 오른쪽 세로형 게이지 위젯으로 접고 다시 클릭해 펼침; 자유 리사이즈.
- **Android 네이티브**: 동일 UI를 Android 앱으로 실행——차트 핀치 줌/팬, 시스템 뒤로 제스처로 이전 페이지, 풀스크린 노치 대응, 브랜드 런처 아이콘.
- **이중 시세 소스**: 시나 파이낸스 / 둥팡차이푸 전환·상호 이중화; 매끄러운 지수 티커; 코드 또는 이름/병음 검색으로 관심종목 추가·삭제.
- **완전 로컬 영속화**: 번들 SQLite, 관심종목/설정/AI 캐시를 모두 저장, 단일 파일로 이전 가능; 중계 서버 없음, 데이터는 기기 내에만.
- **임의의 AI 연결**: 기본 DeepSeek, Base URL/모델을 임의의 OpenAI 호환 서비스(Kimi, 통이, 로컬 Ollama…)로 변경 가능; 키는 기기 내에만 저장.
- **리퀴드 글래스 외관(실험적·설정에서 선택)**: iOS "Liquid Glass" 풍 프로스트——반투명 + 블러 + 하이라이트 테두리 + 오로라 배경 + 플로팅 알약 탭, 주야 대응; 기본 꺼짐, 오래된 Android에서는 불투명으로 자동 강등.

[ArvinLovegood/go-stock](https://github.com/ArvinLovegood/go-stock)(Wails + Go)의 완전 로컬 형태를 참고해 Tauri 2(Rust + 시스템 WebView)로 재구축. Electron 계열보다 번들 크기와 메모리가 훨씬 작다.

## 빠른 시작

### 환경(최초 1회)

- [Rust 툴체인](https://rustup.rs)(Windows는 VS Build Tools의 "C++를 사용한 데스크톱 개발", macOS는 Xcode CLT 필요)
- Windows 10은 [WebView2 런타임](https://developer.microsoft.com/microsoft-edge/webview2/)이 필요할 수 있음(Win11 기본 탑재)
- Tauri CLI: `cargo install tauri-cli --version "^2"`

### 실행

```bash
cd rust-stock
cargo tauri dev      # 개발(프런트 핫 리로드)
cargo tauri build    # 인스톨러 생성(Windows NSIS / macOS dmg)
```

### 프런트만 미리보기(Rust 불필요)

```bash
cd rust-stock/src && python3 -m http.server 8080
# 브라우저에서 http://localhost:8080 ; 데이터는 목(mock)
```

### 테스트

```bash
cd rust-stock/src-tauri && cargo test
# 범위: 시세 파싱(시나/둥팡차이푸), 뉴스 파싱, 심리 알고리즘, SQLite KV
```

## AI 설정(선택)

설정 화면에서 API 키 입력(기본 [DeepSeek](https://platform.deepseek.com); Base URL/모델은 Kimi·통이·로컬 Ollama 등 임의의 OpenAI 호환 서비스로 변경 가능). 키는 기기 내 SQLite에만 저장되고 직접 연결됩니다. 설정하면 관심종목 AI 채점, 심리 AI 해설, AI 채팅이 활성화; 미설정 시에도 우아하게 강등되며 안내를 표시.

## 프로젝트 구조

```
rust-stock/
├── src/                       # 프런트(순수 HTML/CSS/JS + ES modules, 프레임워크/빌드 없음)
│   ├── index.html             # 전체 UI(스타일 인라인)
│   ├── main.js                # 부트스트랩(배선/타이머)
│   └── js/
│       ├── bridge.js          # Tauri 브리지(브라우저 미리보기 강등)
│       ├── store.js           # 전역 상태 + SQLite/localStorage 영속화
│       ├── api.js             # Tauri 명령 래퍼
│       ├── ui.js / router.js  # 공통 부품 / 페이지 전환
│       └── pages/             # 시세 / 뉴스 / 관심종목 / 채팅 / 설정
├── src-tauri/                 # Rust 로컬 로직 계층
│   ├── src/lib.rs             # Tauri 명령 계층(업무 프롬프트 / 창 제어)
│   ├── src/sources/           # 시세 소스 추상화(QuoteSource trait + 레지스트리)
│   ├── src/ai.rs              # AI 공급자 추상화(OpenAI 호환, base_url/model 가변)
│   ├── src/quote.rs           # 시세 모델과 파서(단위 테스트 포함)
│   ├── src/feed.rs            # 뉴스 + 심리 알고리즘(단위 테스트 포함)
│   ├── src/storage.rs         # SQLite KV 영속화(단위 테스트 포함)
│   └── tauri.conf.json        # 창 / 패키지 설정
└── docs/                      # 개발 문서
```

전체 변경 이력은 **[중문 README → 更新日志](README.md#更新日志)**(최신순)에 있습니다.

세부 사항: [개발 문서](rust-stock/docs/DEVELOPMENT.md)

## 면책 조항

시세와 뉴스 데이터는 제3자 공개 API(시나 파이낸스, 둥팡차이푸)에서 가져오며 학습·연구 목적으로만 사용합니다. 상업적 사용 전 라이선스를 직접 확인하세요. AI 분석은 모델 생성물로 참고용일 뿐 투자 자문이 아닙니다. 투자에는 위험이 따르니 신중하시기 바랍니다.

## 감사의 말

- [ArvinLovegood/go-stock](https://github.com/ArvinLovegood/go-stock) — 본 프로젝트의 영감의 원천
- [Tauri](https://tauri.app) · [DeepSeek](https://deepseek.com)

## 라이선스

[Apache License 2.0](LICENSE)
