# x-timeline

hycli에 연결된 X 계정의 로그인 세션으로 북마크, 홈 피드(추천), 홈 피드(팔로잉)를 받아 Markdown과 JSON으로 저장합니다.

X 웹 화면이 사용하는 내부 GraphQL persisted POST를 읽습니다. 공식 API가 아니며 X 웹 배포 때 query ID와 요청 필드가 바뀔 수 있습니다. 이 스크립트는 쓰기 요청을 만들거나 실행하지 않습니다.

## 필요한 것

- 현재 checkout에서 빌드한 `hycli`와 `accounts read --request` 지원
- `hycli accounts`에 `verified_identity: true`, `status: connected`로 연결된 X 계정
- 브라우저 관찰로 각 GraphQL POST route의 성공한 읽기 증거를 저장

관찰은 실제 브라우저의 read-only 요청만 기록합니다. 먼저 `accounts observe <계정 ID> https://x.com/home --browser`를 실행하고, 현재 계정 검증을 마치면 `accounts verify` 결과의 `read_observations`에서 route별 `ro_...` 참조를 확인합니다. 참조는 로컬 vault에만 저장되며 쿠키, storage token, bearer 값은 출력하지 않습니다. 증거가 없거나 body schema가 다르면 `unsupported_evidence`로 중단합니다.

`accounts read --request`는 다음을 검증합니다.

- method, same-origin URL, path, query/body schema가 성공한 브라우저 관찰과 일치
- X persisted GraphQL route와 operation name이 알려진 read route와 일치
- mutation/subscription, change intent, redirect, 비JSON 응답 차단
- `read_only: true`, redirect 미허용, 응답 크기 제한
- 다음 페이지 cursor는 raw 값 대신 계정/요청에 묶인 `rc_...` 핸들로 반환

## 사용법

```bash
# 추천 피드 100개
python3 -I export-x-timeline.py --account n0madamas --timeline home --max-posts 100

# 팔로잉(시간순) 피드 100개
python3 -I export-x-timeline.py --account n0madamas --timeline following --max-posts 100

# 북마크 전체
python3 -I export-x-timeline.py --account n0madamas --timeline bookmarks

# 계정 확인과 첫 페이지만 읽기
python3 -I export-x-timeline.py --account devn0w --timeline home --check
```

- `--account`는 X 핸들입니다. hycli 계정 목록에서 찾고, 같은 핸들이 여러 브라우저 프로필에 연결돼 있으면 가장 최근에 확인된 연결을 씁니다. 특정 연결을 고르려면 `--account-id`를 넘기면 됩니다.
- 추천과 팔로잉은 끝없이 이어지므로 `--max-posts`가 필요합니다.
- 결과는 기본으로 `~/Downloads/x-<timeline>-<핸들>-<시각>/`에 저장됩니다. `--output-base`나 `--output`으로 바꿀 수 있습니다.

## 결과 폴더

| 파일 | 내용 |
|---|---|
| `01-<timeline>.md` | 읽기용 목록(작성자, 날짜, 링크, 본문) |
| `02-<timeline>.json` | 정리된 게시물(본문, 외부 링크, 미디어 URL)과 읽지 못한 항목 |
| `03-manifest.json` | 페이지별 개수, 광고와 추천 모듈 제외 수, 종료 근거, 오류 |
| `04-pages/` | `accounts read`의 상태/계정 ID/JSON body/opaque next cursor를 담은 페이지 응답 |
| `05-export.py`, `06-request.json` | 실행한 스크립트와 POST 요청 descriptor 사본, observation reference 포함 |

광고(`promotedMetadata`)와 팔로우 추천 같은 게시물이 아닌 모듈은 빼고 개수만 기록합니다.

## 팔로우 데이터의 의미

`following`은 팔로우한 계정 목록이 아니라 X의 `HomeLatestTimeline` 팔로잉 피드입니다. 팔로우 계정 목록은 별도의 성공한 read-only 관찰 증거가 없으므로 이 exporter가 대신 수집한다고 주장하지 않습니다.

## 요청 제한

- X가 429를 보내면(`site_rate_limited`) 다시 시도하지 않고 바로 멈춥니다. 그때까지 받은 페이지는 저장되고 `03-manifest.json`의 `error`에 이유가 남습니다. X의 제한 창은 15분입니다.
- hycli 자체 상한(`rate_limited`, 10분에 60회, 요청 간 최소 1.2초)에 걸리면 X에는 요청이 나가지 않은 상태이므로 `--local-wait`초(기본 30초) 기다린 뒤 이어서 받습니다.
- hycli의 직접 요청은 TLS 지문, User-Agent(`Linux Chrome 134`), X 웹 전용 헤더가 실제 브라우저와 다릅니다. 실행 횟수를 줄이고, 여러 계정을 연달아 돌리지 않는 편이 안전합니다.

## 쿼리 ID가 바뀌었을 때

X 응답이 404나 GraphQL 오류로 바뀌면 쿼리 ID나 `features` 목록이 바뀐 것입니다.

1. 로그인한 브라우저에서 `https://x.com/home`을 엽니다. 예전 웹앱(`responsive-web`)이 로드된 상태여야 합니다.
2. 개발자 도구 콘솔에서 로드된 모듈 중 해당 `operationName` 정의를 찾습니다.

    ```js
    (window.webpackChunk_twitter_responsive_web || []).flatMap(c => Object.values(c[1] || {}))
      .map(String).map(s => s.match(/queryId:"[^"]+",operationName:"(HomeLatestTimeline|HomeTimeline|Bookmarks)"[\s\S]{0,4000}?\}\}/))
      .filter(Boolean).map(m => m[0])
    ```

3. `queryId`는 `x-graphql.json`의 `path`에, `featureSwitches` 목록은 `features`의 키에 반영합니다.
4. 바뀐 주소를 `hycli accounts observe`로 다시 기록하고 `--check`로 확인합니다.

비로그인 페이지는 새 웹앱(`x-web`, Relay 방식)을 받습니다. 거기서 보이는 `HomeLatestTimelineQuery` 같은 ID는 다른 체계라 이 스크립트에 쓸 수 없습니다.

## 테스트

```bash
python3 -I test_export.py
```

가짜 `hycli`로 429 중단, 자체 상한 대기, 개수 제한, 광고와 추천 모듈 제외, 계정 선택, 주소별 query 키를 확인합니다. X에는 요청하지 않습니다.
