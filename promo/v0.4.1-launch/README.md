# soon v0.4.1 launch kit

This directory is the source of truth for the v0.4.1 launch campaign. It keeps
the message, artwork, demo, and measurement plan together so the launch can be
reproduced without uploading command history or inventing vanity metrics.

Nothing in this directory publishes a post. Public posting requires an explicit
confirmation after the links and rendered assets have been checked.

## One-line promise

**soon predicts the next full Zsh command locally, repairs failed commands, and
never presses Enter for you.**

## Links

- Repository: https://github.com/HsiangNianian/soon
- v0.4.1 prerelease: https://github.com/HsiangNianian/soon/releases/tag/v0.4.1
- Zsh pilot cohort: https://github.com/HsiangNianian/soon/issues/27
- Launch tracker: https://github.com/HsiangNianian/soon/issues/32
- Website: https://soon.hydroroll.team

## Rendered assets

Run `./render.sh` to regenerate every binary asset from the SVG sources.

| Asset | Size | Primary use |
| --- | ---: | --- |
| `rendered/repair-receipt-1280x720.png` | 1280×720 | Show HN, Reddit, link previews |
| `rendered/terminal-proof-1080x1080.png` | 1080×1080 | X post, square social card |
| `rendered/workflow-strip-1500x500.png` | 1500×500 | X profile/header campaign banner |
| `rendered/soon-v0.4.1-demo-1280x720.mp4` | 1280×720, 15s | Native video upload |
| `rendered/soon-v0.4.1-demo-960x540.gif` | 960×540, 15s | Embeddable fallback |

Suggested alt text:

> A Zsh terminal shows a mistyped `git psuh` command fail. soon suggests
> `git push` from local history in milliseconds. Ctrl-F places it into the
> editable command buffer; nothing executes automatically. After success, soon
> suggests `gh pr checks --watch` as the next step.

## Channel copy

### Show HN

**Title**

> Show HN: soon – local Zsh command prediction that never presses Enter

**Body**

> I built soon because shell autocomplete usually completes the token I am
> typing, while I often want the next full command in a workflow.
>
> v0.4.1 adds an opt-in Zsh loop: after a command succeeds it can suggest the
> next step; after a failure it can suggest a repair. The default path uses
> local history and a deterministic ranker, normally returns in a few
> milliseconds, and does not call a model or network service. Ctrl-F copies the
> ghost text into the editable buffer. soon never runs it for you.
>
> Install with `cargo install soon` or `python -m pip install soon-bin`, then
> run `eval "$(soon init zsh)"`. I am looking for Zsh users willing to try the
> privacy-safe pilot and share aggregate feedback—never raw commands:
> https://github.com/HsiangNianian/soon/issues/27
>
> Code and the v0.4.1 prerelease:
> https://github.com/HsiangNianian/soon

### Reddit / r/commandline

**Title**

> soon v0.4.1: local next-command and failed-command repair suggestions for Zsh

**Body**

> I have been working on `soon`, a local-first command predictor for Zsh. It
> learns workflow transitions from your own shell history, suggests a full next
> command after success, and can repair a failed command such as
> `git psuh → git push`.
>
> Suggestions are ghost text. Ctrl-F only places one into the editable buffer;
> it never executes automatically. The default predictor is deterministic and
> local—no model and no network call—and sensitive-looking commands are
> filtered before learning.
>
> v0.4.1 is available from Cargo and PyPI. I would especially value feedback on
> relevance, latency, and whether the interaction stays out of the way:
> https://github.com/HsiangNianian/soon
>
> The ten-user Zsh pilot records aggregate counters, not raw commands:
> https://github.com/HsiangNianian/soon/issues/27

### V2EX

**标题**

> [开源] soon v0.4.1：在本地预测下一条 Zsh 命令，也能修复刚输错的命令

**正文**

> 我做了一个叫 soon 的本地命令预测工具。和补全当前单词不同，它会根据你自己的终端历史，
> 在命令成功后预测工作流的下一步，也会在失败后给出修复建议，例如
> `git psuh → git push`。
>
> 建议以灰色 ghost text 出现；按 Ctrl-F 只是把它放进可编辑的命令行，是否修改、执行或清空
> 仍由你决定，soon 不会自动按 Enter。默认预测器是确定性的本地算法，不调用模型，也不访问
> 网络；疑似敏感的命令会在学习前过滤。
>
> v0.4.1 已可从 Cargo 和 PyPI 安装：
>
> `cargo install soon`
>
> 或 `python -m pip install soon-bin`
>
> 项目地址：https://github.com/HsiangNianian/soon
>
> 我们正在招募 10 位 Zsh 用户做隐私安全的试用，只提交聚合计数，不收原始命令：
> https://github.com/HsiangNianian/soon/issues/27

### X

**Single post**

> `git psuh` failed. soon suggested `git push` locally in 6.8 ms.
>
> Ctrl-F puts it in the editable Zsh buffer. It never presses Enter.
>
> v0.4.1 also predicts the next full command after success—no model or network
> on the default path.
>
> https://github.com/HsiangNianian/soon

**Optional reply**

> I am looking for Zsh users for a privacy-safe pilot. We collect aggregate
> counters, not command text: https://github.com/HsiangNianian/soon/issues/27

## Measurement

GitHub traffic is a rolling 14-day window. Clone counts may include CI,
packaging, mirrors, and bots, so they are recorded as noisy diagnostics rather
than treated as users.

| Checkpoint | Observed at (UTC) | Stars | Repo views | Unique viewers | Clones (noisy) | Unique cloners (noisy) | External referrers | Pilot volunteers |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Baseline | 2026-07-29T02:57:16Z | 8 | 159 | 26 | 1,142 | 154 | 2 | 0 |
| +24h | — | — | — | — | — | — | — | — |
| +72h | — | — | — | — | — | — | — | — |

Baseline referrers were `github.com` (4 unique visitors) and
`soon.hydroroll.team` (1 unique visitor).

## Publication gate

Before posting anywhere:

- [ ] Re-open every link in this document
- [ ] Confirm Cargo, PyPI, and the GitHub prerelease still report v0.4.1
- [ ] Watch the MP4 and GIF once from beginning to end
- [ ] Confirm #27 still accepts pilot volunteers and has not changed scope
- [ ] Capture a fresh star/traffic baseline if more than 24 hours have passed
- [ ] Get explicit confirmation for the named public channels

After posting, add the exact post URLs to issue #32 and fill the +24h and +72h
rows above. Do not infer pilot participants from stars or clone counts.
