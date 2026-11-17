<script lang="ts">
  // 课程视图：左侧课程列表 + 右侧阅读器 + 三厂商命令对照表（Profile 级纯读 inspection）。
  import { LESSONS, lessonTitle, lessonMarkdown } from '../lib/course';
  import { t, i18n } from '../lib/i18n.svelte';
  import {
    CMDMAP,
    profileForRow,
    rowMode,
    type CmdRow,
    type CmdVendor,
  } from '../content/cmdmap';
  import { renderMarkdown } from '../lib/md';
  import { getSim, inspectCommand, type InspectionResult } from '../lib/sim';
  import { app } from '../lib/store.svelte';
  import { LABS, labTitle } from '../lib/labs';

  let currentId = $state(LESSONS[0]?.id ?? '');
  const current = $derived(LESSONS.find((l) => l.id === currentId) ?? LESSONS[0]);

  const LEVELS = [
    { id: 'IA', key: 'course.level.ia' },
    { id: 'IP', key: 'course.level.ip' },
    { id: 'IE', key: 'course.level.ie' },
    { id: 'MAP', key: 'course.level.map' },
  ] as const;
  const groups = $derived(
    LEVELS.map((lv) => ({
      ...lv,
      lessons: LESSONS.filter((l) => l.level === lv.id),
    })).filter((g) => g.lessons.length > 0)
  );
  const html = $derived(
    current ? renderMarkdown(lessonMarkdown(current, i18n.lang === 'en')) : ''
  );

  // 对照表按当前课程主题过滤
  const rows = $derived(
    current ? CMDMAP.filter((r) => r.topic === current.topic) : []
  );

  const VENDORS: { id: CmdVendor; key: string }[] = [
    { id: 'ios', key: 'course.ios' },
    { id: 'vrp', key: 'course.vrp' },
    { id: 'comware', key: 'course.comware' },
    // 阶段 5A：F5 TMOS 17.x 独立第四列；LTM 行只在这一列有真实命令。
    { id: 'tmos', key: 'course.tmos' },
  ];

  /**
   * 缓存 key 至少包含 Profile + 模式 + 命令；不含界面语言，语言切换不污染结果。
   * 结构化结果来自 sim.inspect_command（平台级纯读），旧 boolean verify_command 只保留兼容。
   */
  const inspectCache = new Map<string, InspectionResult>();

  function inspect(row: CmdRow, vendor: CmdVendor): InspectionResult {
    const cmd = row[vendor] ?? '';
    const mode = rowMode(row, vendor);
    const profileId = profileForRow(row, vendor);
    const key = `${profileId}|${mode}|${cmd}`;
    const cached = inspectCache.get(key);
    if (cached) return cached;
    try {
      const r = inspectCommand(getSim(), profileId, mode, cmd);
      inspectCache.set(key, r);
      return r;
    } catch (e) {
      // 异常不写缓存，避免把一次 WASM 错误固化成另一种命令状态。
      return {
        passed: false,
        detail: String(e),
        status: 'InvalidCommand',
        support: 'invalid-command',
        evidence: 'Unverified',
        specId: '',
      };
    }
  }

  /**
   * 阶段 4A：支持状态完全由结构化 support/status/evidence 决定，不再按厂商一刀切降级。
   * Comware 首批已闭环且有官方证据的命令由 inspect_command 返回 full-support；
   * 未迁移、未模拟或证据不足的命令由规格表返回 pending-verification / recognized-only，
   * security 等未收口主题仍不得显示完整支持。
   */
  function supportKey(r: InspectionResult): string {
    switch (r.support) {
      case 'full-support':
        return 'course.support.full';
      case 'recognized-only':
        return 'course.support.recognized';
      case 'unsupported-by-device':
        return 'course.support.unsupported';
      case 'invalid-command':
        return 'course.support.invalid';
      default:
        return 'course.support.pending';
    }
  }

  function markOf(r: InspectionResult): string {
    switch (r.support) {
      case 'full-support':
        return '✓';
      case 'recognized-only':
        return '◐';
      case 'unsupported-by-device':
        return '⊘';
      case 'invalid-command':
        return '✗';
      default:
        return '?';
    }
  }

  function stateClass(r: InspectionResult): string {
    return r.support;
  }

  // 浏览器验收钩子：暴露真实缓存 key 与结构化结果，用于验证不串厂商缓存。
  const host = window as unknown as { __netemu?: Record<string, unknown> };
  host.__netemu = {
    ...(host.__netemu ?? {}),
    cmdmapInspections: () =>
      [...inspectCache.entries()].map(([key, r]) => ({ key, ...r })),
  };

  function openLab() {
    if (!current?.lab) return;
    app.view = 'lab';
    window.dispatchEvent(new CustomEvent('netemu:open-lab', { detail: current.lab }));
  }
</script>

<div class="course-view">
  <aside class="list panel">
    <h3>{t('course.catalog')}</h3>
    <div class="pad lessons">
      {#each groups as g (g.id)}
        <div class="level-head">
          <span class="level-badge">{g.id}</span>
          {t(g.key)}
          <span class="level-count">{g.lessons.length}</span>
        </div>
        {#each g.lessons as l (l.id)}
          <button class="lesson" class:active={l.id === currentId} onclick={() => (currentId = l.id)}>
            <span class="no">{String(l.order).padStart(2, '0')}</span>
            {lessonTitle(l, i18n.lang === 'en')}
          </button>
        {/each}
      {/each}
    </div>
  </aside>

  <div class="reader">
    {#if current}
      <div class="panel lesson-card">
        <div class="pad">
          <h1 class="lesson-title">{lessonTitle(current, i18n.lang === 'en')}</h1>
          <div class="md">{@html html}</div>
          {#if current.lab}
            {@const lab = LABS.find((x) => x.id === current.lab)}
            <button class="primary lab-open" onclick={openLab}>
              {t('course.openLab')}{lab ? `: ${labTitle(lab)}` : ''}
            </button>
          {/if}
        </div>
      </div>

      {#if rows.length}
        <div class="panel">
          <h3>{t('course.table')}</h3>
          <div class="pad table-wrap">
            <div class="table-note">{t('course.platform')} · {t('course.pureRead')}</div>
            <table class="cmd-table mono">
              <thead>
                <tr>
                  <th>{t('course.feature')}</th>
                  {#each VENDORS as v (v.id)}<th>{t(v.key)}</th>{/each}
                </tr>
              </thead>
              <tbody>
                {#each rows as row (row.feature)}
                  <tr>
                    <td class="feat">{row.feature}</td>
                    {#each VENDORS as v (v.id)}
                      {#if row[v.id]}
                        {@const r = inspect(row, v.id)}
                        <td class="cell {stateClass(r)}">
                          {#if row.teachingVendors?.includes(v.id)}
                            <div class="teaching-badge">{t('course.teaching')}</div>
                          {/if}
                          <div class="state-line">
                            <span class="mark">{markOf(r)}</span>
                            <span class="support">{t(supportKey(r))}</span>
                            <span class="status">{t('course.status')}: {r.status}</span>
                          </div>
                          <div class="cmd">{row[v.id]}</div>
                          <div class="meta">
                            {t('course.evidence')}: {r.evidence}{r.specId ? ` · ${r.specId}` : ''}
                          </div>
                          <div class="meta detail">{r.detail}</div>
                        </td>
                      {:else}
                        <td class="cell not-applicable">
                          <div class="meta">{t('course.notApplicable')}</div>
                        </td>
                      {/if}
                    {/each}
                  </tr>
                {/each}
              </tbody>
            </table>
            <div class="hint">{t('course.tableHint')}</div>
            <div class="hint comware-note">{t('course.comwareNotice')}</div>
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .course-view {
    height: 100%;
    display: flex;
    min-height: 0;
  }
  .list {
    width: 240px;
    min-width: 200px;
    margin: 12px 0 12px 12px;
    align-self: flex-start;
    max-height: calc(100% - 24px);
    overflow-y: auto;
  }
  .lessons {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px;
  }
  .level-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 2px 2px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .level-head:first-child {
    margin-top: 0;
  }
  .level-badge {
    font-family: 'Maple Mono', Consolas, monospace;
    font-size: 11px;
    padding: 1px 5px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    color: var(--accent);
  }
  .level-count {
    margin-left: auto;
    font-family: 'Maple Mono', Consolas, monospace;
    opacity: 0.7;
  }
  .lesson {
    text-align: left;
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .lesson.active {
    border-color: var(--accent);
    color: var(--accent);
  }
  .lesson .no {
    font-family: 'Maple Mono', Consolas, monospace;
    color: var(--text-dim);
    font-size: 12px;
  }
  .reader {
    flex: 1;
    min-width: 0;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow-y: auto;
    max-height: 100%;
  }
  .lesson-card .pad {
    max-width: 42em;
    padding: 20px 26px 24px;
  }
  .lesson-title {
    margin: 0 0 14px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
    font-size: 22px;
    line-height: 1.4;
    letter-spacing: 0.01em;
  }
  .lab-open {
    margin-top: 18px;
    width: 100%;
    white-space: normal;
    text-align: left;
  }
  .table-wrap {
    padding: 0 16px 16px;
    overflow-x: auto;
  }
  .table-note {
    font-size: 12px;
    color: var(--accent);
    padding: 10px 0 8px;
  }
  .cmd-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  .cmd-table th,
  .cmd-table td {
    border: 1px solid var(--border);
    padding: 7px 9px;
    text-align: left;
    vertical-align: top;
  }
  .cmd-table th {
    color: var(--text-dim);
    font-weight: 600;
    white-space: nowrap;
  }
  .cmd-table .feat {
    color: var(--text);
    font-family: inherit;
    white-space: nowrap;
  }
  .cmd-table .cell {
    min-width: 220px;
  }
  .state-line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
    margin-bottom: 5px;
  }
  .state-line .mark {
    font-size: 14px;
    font-weight: 700;
  }
  .state-line .support {
    font-weight: 600;
  }
  .state-line .status {
    font-size: 10.5px;
    color: var(--text-dim);
  }
  .cmd {
    white-space: normal;
    word-break: break-word;
    color: var(--text);
    margin-bottom: 4px;
  }
  .meta {
    font-size: 10.5px;
    color: var(--text-dim);
    line-height: 1.4;
  }
  .meta.detail {
    margin-top: 2px;
  }
  .full-support .mark,
  .full-support .support {
    color: var(--good);
  }
  .recognized-only .mark,
  .recognized-only .support,
  .pending-verification .mark,
  .pending-verification .support {
    color: #d1a03a;
  }
  .unsupported-by-device .mark,
  .unsupported-by-device .support,
  .invalid-command .mark,
  .invalid-command .support {
    color: var(--bad);
  }
  .teaching-badge {
    display: inline-block;
    margin-bottom: 5px;
    padding: 1px 6px;
    border: 1px solid #d1a03a;
    border-radius: 4px;
    color: #d1a03a;
    font-size: 10.5px;
    font-weight: 600;
  }
  .not-applicable {
    color: var(--text-dim);
    background: rgba(127, 127, 127, 0.06);
  }
  .comware-note {
    margin-top: 4px;
  }
  .hint {
    color: var(--text-dim);
    font-size: 12px;
    margin-top: 8px;
    line-height: 1.5;
  }
</style>
