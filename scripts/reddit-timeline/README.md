# reddit-timeline

hycli에 연결된 Reddit 로그인 세션으로 홈 피드와 저장한 글(댓글 포함)을 받아 Markdown과 JSON으로 저장합니다. 결과 형식과 요청 제한 처리는 `../x-timeline`과 같습니다.

Reddit 웹 주소에 `.json`을 붙인 목록 주소(`/.json`, `/user/<이름>/saved.json`)를 로그인 쿠키로 GET합니다. 주소 체계는 [rdt-cli](https://github.com/public-clis/rdt-cli)에서 확인했습니다. Reddit 공식 API는 2025년 11월부터 새 앱에 사전 승인을 요구하고 개인 스크립트는 승인받기 어렵다는 보고가 많아 쓰지 않았습니다. 이 방식도 Reddit 약관의 자동 수집 제한에 걸릴 수 있습니다.

## rdt-cli 대신 이 스크립트를 쓰는 이유

- hycli 금고에 있는 여러 브라우저 프로필 세션 중에서 Reddit 계정 이름으로 고릅니다. rdt-cli는 Chrome 기본 프로필의 쿠키만 읽습니다.
- 쿠키를 다른 파일에 한 벌 더 저장하지 않습니다.
- Reddit이 429를 보내면 바로 멈춥니다.

## 필요한 것

- `hycli accounts read`가 들어간 hycli 빌드. 2026-10-20 기준 이 명령은 main에 없고, 로컬 브랜치 `fix/session-cli`(커밋 `d82b8d8`)에만 있습니다.
- hycli에 Reddit 세션이 들어와 있어야 합니다. 브라우저 프로필에서 가져오려면 `hycli browsers --url https://www.reddit.com`을 실행합니다. 이 명령은 Reddit에 로그인된 모든 Chrome 프로필의 쿠키를 hycli 로컬 금고로 가져옵니다.

계정 확인과 주소 기록은 스크립트가 알아서 합니다.

- **계정 확인**: 요청한 이름이 확인된 계정 중에 없으면, 확인 전인 프로필을 하나씩 `hycli accounts verify`로 확인해 찾습니다. 확인 설정은 `reddit.json`의 `identity_recipe`입니다.
    - `accounts verify`는 쿠키 없는 비교 요청도 함께 보냅니다. Reddit은 이 요청을 제한 응답(429 또는 `Retry-After`)으로 돌려주고, 그러면 hycli가 reddit.com 전체에 대기를 겁니다(기본 60초). 그래서 처음 확인할 때만 이 명령을 씁니다.
    - 이미 확인된 계정은 쿠키를 붙인 `/api/me.json` 읽기 한 번으로 이름과 ID만 대조합니다.
- **주소 기록**: hycli는 관찰된 주소와 query 키 묶음만 읽게 해 줍니다. 처음 보는 주소(계정마다 다른 `saved` 주소 등)는 `hycli accounts observe`로 한 번 기록한 뒤 읽습니다. 이때 같은 주소에 요청이 한 번 더 나갑니다.
- **query 키 고정**: 첫 페이지에도 `after`를 빈 값으로 보내 키 묶음(`limit`, `after`, `raw_json`)을 고정합니다.

## 사용법

```bash
# 홈 피드 100개
python3 -I export-reddit-timeline.py --account swonme --timeline home --max-posts 100

# 저장한 글 전체 (Reddit이 최근 약 1,000개까지만 돌려줍니다)
python3 -I export-reddit-timeline.py --account swonme --timeline saved

# 계정 확인과 첫 페이지만 읽기. 이름을 모르는 프로필은 hycli 계정 id로 지정합니다
python3 -I export-reddit-timeline.py --account-id <hycli 계정 id> --timeline home --check
```

- 같은 Reddit 계정이 여러 프로필에 연결돼 있으면 가장 최근에 확인된 연결을 씁니다.
- 결과는 기본으로 `~/Downloads/reddit-<timeline>-<이름>-<시각>/`에 저장됩니다.

## 결과 폴더

| 파일 | 내용 |
|---|---|
| `01-<timeline>.md` | 읽기용 목록(제목, 서브레딧, 작성자, 점수, 본문, 외부 링크) |
| `02-<timeline>.json` | 정리된 항목. 글(`post`)은 이미지, 갤러리, 동영상 URL까지, 댓글(`comment`)은 원글 제목과 본문 |
| `03-manifest.json` | 페이지별 개수, 광고와 글이 아닌 항목 제외 수, 종료 근거, 오류 |
| `04-pages/` | Reddit 응답 원본 |
| `05-export.py`, `06-request.json` | 실행한 스크립트와 요청 사본 |

## 요청 제한

- Reddit이 429를 보내면(`site_rate_limited`) 다시 시도하지 않고 멈춥니다. 그때까지 받은 페이지는 저장됩니다.
- hycli 자체 상한(`rate_limited`)에 걸리면 Reddit에는 요청이 나가지 않은 상태이므로 `--local-wait`초(기본 30초) 기다린 뒤 이어서 받습니다.

## 테스트

```bash
python3 -I test_export.py
```

가짜 `hycli`로 429 중단, 자체 상한 대기, query 키 고정, 광고 제외, 새 주소 기록, 댓글 정리, 확인 전 프로필 탐색, 확인된 계정의 비교 요청 생략을 확인합니다. Reddit에는 요청하지 않습니다.
