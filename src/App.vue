<template>
  <div class="wrap">
    <div v-if="scanning" class="modal-mask">
      <div class="modal">
        <h3>正在扫描…</h3>
        <div class="pbar"><div class="pfill" :style="{ width: pct + '%' }"></div></div>
        <p class="pname" :title="progress.name">{{ progress.name }}</p>
        <p class="pnum">{{ progress.current }} / {{ progress.total }} 个包</p>
      </div>
    </div>

    <!-- 检查更新结果 -->
    <div v-if="updateInfo.show" class="modal-mask" @click.self="updateInfo.show = false">
      <div class="modal">
        <h3>检查更新</h3>
        <template v-if="updateInfo.latest">
          <p v-if="isNewer(updateInfo.latest, appVersion)" class="upd-new">
            发现新版本 <b>v{{ updateInfo.latest }}</b>（当前 v{{ appVersion }}）
          </p>
          <p v-else class="upd-ok">已是最新版本（v{{ appVersion }}）</p>
          <div class="row-btns">
            <button v-if="isNewer(updateInfo.latest, appVersion)" class="primary" @click="openPage(updateInfo.url)">前往下载页</button>
            <button class="ghost" @click="updateInfo.show = false">关闭</button>
          </div>
        </template>
        <p v-else class="upd-err">{{ updateInfo.err }}</p>
      </div>
    </div>

    <!-- 发布新版 -->
    <div v-if="pub.show" class="modal-mask" @click.self="!pub.busy && (pub.show = false)">
      <div class="modal left">
        <h3>发布新版</h3>
        <p class="pub-hint">提交 → 推送 → 打附注标签，GitHub Actions 自动构建并发布。版本号由标签自动同步，无需改配置。</p>
        <label class="f-label">项目目录</label>
        <input class="f-input" readonly :value="pubStatus.projectDir || '未找到仓库（发布功能仅限开发机）'" />
        <label class="f-label">发布版本</label>
        <input v-model="pub.version" class="f-input" placeholder="0.2.2" />
        <p class="status-line">
          <template v-if="pubStatus.err">⚠ {{ pubStatus.err }}</template>
          <template v-else>
            最新标签 <b>{{ pubStatus.latestTag || "无" }}</b> · 分支 {{ pubStatus.branch || "?" }} ·
            <span :class="pubStatus.dirty ? 'dirty' : 'clean'">{{ pubStatus.dirty ? `工作区有 ${pubStatus.dirty} 处改动` : "工作区无改动" }}</span>
          </template>
        </p>
        <label class="f-label">提交说明</label>
        <input v-model="pub.commitMsg" class="f-input" placeholder="chore: release v0.2.2" />
        <label class="f-label">发布说明（可选：写入标签注释，显示在 GitHub Release 页面；留空则用固定文案）</label>
        <textarea v-model="pub.notes" class="f-area" rows="3" placeholder="本次更新内容…"></textarea>
        <div class="row-btns">
          <button class="ghost" :disabled="pub.busy" @click="refreshStatus">刷新状态</button>
          <button class="ghost" :disabled="pub.busy" @click="doPublish(true)">试运行</button>
          <button class="primary" :disabled="pub.busy" @click="doPublish(false)">{{ pub.busy ? "推送中…" : "发布新版本" }}</button>
          <button class="ghost" :disabled="pub.busy" @click="pub.show = false">关闭</button>
        </div>
        <label class="f-label">执行日志（{{ logs.length }} 条）</label>
        <div class="log-panel">
          <div v-for="(l, i) in logs" :key="i" class="log-line"><span class="log-time">{{ l.time }}</span>{{ l.msg }}</div>
          <div v-if="!logs.length" class="log-line log-time">就绪。填写版本号后点击「试运行」或「发布新版本」。</div>
        </div>
      </div>
    </div>

    <header class="hd">
      <div>
        <h1>幕府将军2 Mod 冲突检测器</h1>
        <p class="sub">按游戏真实加载顺序（boot → release → patch → mod → movie）解析 pack 内 db 表，检测同名表覆盖冲突与科技生态悬空引用（加载页闪退高发原因）</p>
      </div>
      <div class="tools">
        <span class="ver">v{{ appVersion }}</span>
        <button class="ghost" :disabled="checking" @click="checkUpdate">{{ checking ? "检查中…" : "检查更新" }}</button>
        <button class="ghost" @click="openPublish">发布新版</button>
      </div>
    </header>

    <section class="bar">
      <label>游戏根目录</label>
      <input v-model="gameDir" spellcheck="false" placeholder="D:/Program Files/Steam/steamapps/common/Total War SHOGUN 2" />
      <label class="chk"><input type="checkbox" v-model="includeAll" /> 扫描全部 mod（含未启用）</label>
      <button :disabled="scanning" @click="runScan">{{ scanning ? "扫描中…" : "开始扫描" }}</button>
    </section>

    <p v-if="error" class="err-bar">⚠ {{ error }}</p>

    <template v-if="report">
      <section class="cards">
        <div class="card"><b>{{ report.packsTotal }}</b><span>扫描包</span></div>
        <div class="card ok"><b>{{ report.packsOk }}</b><span>健康</span></div>
        <div class="card" :class="{ bad: report.packsBad > 0 }"><b>{{ report.packsBad }}</b><span>异常包</span></div>
        <div class="card"><b>{{ report.modsEnabled }}</b><span>启用 mod</span></div>
        <div class="card" :class="{ bad: report.errors > 0 }"><b>{{ report.errors }}</b><span>严重</span></div>
        <div class="card" :class="{ warn: report.warnings > 0 }"><b>{{ report.warnings }}</b><span>警告</span></div>
      </section>
      <p class="meta">扫描时间：{{ report.scannedAt }}　目录：{{ report.gameDir }}</p>

      <section class="block">
        <h2>一、科技生态完整性 <small>悬空引用 = 加载页闪退高发</small></h2>
        <ul class="tech">
          <li v-for="(t, i) in report.tech" :key="i" :class="t.level.toLowerCase()">
            <span class="tag" :class="t.level.toLowerCase()">{{ levelText(t.level) }}</span>{{ t.msg }}
          </li>
          <li v-if="!report.tech.length" class="info">未检测到科技表覆盖</li>
        </ul>
      </section>

      <section class="block">
        <h2>二、包与表结构健康</h2>
        <table v-if="report.findings.length">
          <thead><tr><th style="width:70px">级别</th><th style="width:280px">包</th><th>问题</th></tr></thead>
          <tbody>
            <tr v-for="(f, i) in report.findings" :key="i">
              <td><span class="tag" :class="f.level.toLowerCase()">{{ levelText(f.level) }}</span></td>
              <td>{{ f.pack }}</td>
              <td class="mono">{{ f.msg }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="ok-text">✔ 全部包结构正常，无异常表</p>
      </section>

      <section class="block">
        <h2>三、同名表覆盖冲突 <small>同 key 行按优先级取一个（列表靠前 / Movie 优先）</small></h2>
        <table v-if="report.conflicts.length">
          <thead><tr><th style="width:70px">级别</th><th style="width:320px">覆盖链（前 → 后）</th><th style="width:280px">db 表</th><th>结论</th></tr></thead>
          <tbody>
            <tr v-for="(c, i) in report.conflicts" :key="i">
              <td><span class="tag" :class="c.level.toLowerCase()">{{ levelText(c.level) }}</span></td>
              <td class="mono small">{{ c.chain.join(" → ") }}</td>
              <td class="mono small">{{ c.table }}</td>
              <td class="small">{{ c.note }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="ok-text">✔ 无多包同名表覆盖</p>
      </section>
    </template>

    <p v-else-if="!error" class="hint">点击「开始扫描」检测当前启用的 MOD 冲突情况</p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import type { ScanReport } from "./types";

const gameDir = ref("");
const includeAll = ref(false);
const scanning = ref(false);
const report = ref<ScanReport | null>(null);
const error = ref("");
const progress = ref({ current: 0, total: 0, name: "" });
const appVersion = ref("");
const checking = ref(false);
const updateInfo = ref<{ show: boolean; latest: string; url: string; err: string }>({
  show: false, latest: "", url: "", err: "",
});
const pub = ref({ show: false, version: "", commitMsg: "", notes: "", busy: false });
const pubStatus = ref<{ projectDir: string; latestTag: string; branch: string; dirty: number; nextVersion: string; err: string }>({
  projectDir: "", latestTag: "", branch: "", dirty: 0, nextVersion: "", err: "",
});
const logs = ref<{ time: string; msg: string }[]>([]);

function addLog(msg: string) {
  const t = new Date();
  const time = [t.getHours(), t.getMinutes(), t.getSeconds()].map((x) => String(x).padStart(2, "0")).join(":");
  logs.value.push({ time, msg });
}

const pct = computed(() =>
  progress.value.total ? Math.round((progress.value.current / progress.value.total) * 100) : 0
);

function isNewer(latest: string, current: string): boolean {
  const p = (s: string) => s.replace(/^v/, "").split(".").map((x) => parseInt(x) || 0);
  const a = p(latest), b = p(current);
  for (let i = 0; i < 3; i++) {
    if ((a[i] || 0) !== (b[i] || 0)) return (a[i] || 0) > (b[i] || 0);
  }
  return false;
}

async function checkUpdate() {
  checking.value = true;
  try {
    const res = await invoke<string>("check_update");
    const { tag, url } = JSON.parse(res);
    updateInfo.value = { show: true, latest: tag.replace(/^v/, ""), url, err: "" };
  } catch (e) {
    updateInfo.value = { show: true, latest: "", url: "", err: String(e) };
  } finally {
    checking.value = false;
  }
}

function openPage(url: string) {
  invoke("open_release_page", { url }).catch(() => {});
}

async function refreshStatus() {
  try {
    const s = await invoke<any>("repo_status");
    pubStatus.value = { ...s, err: "" };
    if (!pub.value.version) pub.value.version = s.nextVersion;
    if (!pub.value.commitMsg) pub.value.commitMsg = `chore: release v${s.nextVersion}`;
  } catch (e) {
    pubStatus.value = { ...pubStatus.value, err: String(e) };
  }
}

function openPublish() {
  pub.value = { show: true, version: "", commitMsg: "", notes: "", busy: false };
  logs.value = [];
  refreshStatus();
}

async function doPublish(dryRun: boolean) {
  if (!pub.value.version.trim() || !pub.value.commitMsg.trim()) {
    addLog("错误：版本号与提交说明都需要填写");
    return;
  }
  pub.value.busy = true;
  try {
    const msg = await invoke<string>("publish_release", {
      version: pub.value.version.trim(),
      commitMsg: pub.value.commitMsg.trim(),
      notes: pub.value.notes.trim(),
      dryRun,
    });
    addLog(msg);
  } catch (e) {
    addLog("失败：" + String(e));
  } finally {
    pub.value.busy = false;
    refreshStatus();
  }
}

onMounted(() => {
  invoke<string>("default_game_dir").then((d) => { if (d) gameDir.value = d; }).catch(() => {});
  getVersion().then((v) => { appVersion.value = v; }).catch(() => {});
  listen<{ current: number; total: number; name: string }>("scan-progress", (e) => {
    progress.value = e.payload;
  }).catch(() => {});
  listen<string>("publish-log", (e) => {
    addLog(e.payload);
  }).catch(() => {});
});

async function runScan() {
  scanning.value = true;
  error.value = "";
  progress.value = { current: 0, total: 0, name: "准备中…" };
  try {
    report.value = await invoke<ScanReport>("scan_game", {
      gameDir: gameDir.value.trim(),
      includeAll: includeAll.value,
    });
  } catch (e) {
    error.value = String(e);
    report.value = null;
  } finally {
    scanning.value = false;
  }
}

function levelText(l: string): string {
  return { ERROR: "严重", WARN: "警告", INFO: "提示", OK: "正常" }[l] ?? l;
}
</script>

<style>
* { box-sizing: border-box; margin: 0; padding: 0; }
body { font-family: "Microsoft YaHei", sans-serif; background: #f4f5f7; color: #2c3e50; }
.wrap { padding: 20px 24px; max-width: 1160px; margin: 0 auto; }
h1 { font-size: 20px; margin-bottom: 4px; }
h2 { font-size: 15px; margin: 0 0 10px; border-bottom: 2px solid #dfe3e8; padding-bottom: 5px; }
h2 small { font-weight: normal; color: #888; margin-left: 8px; font-size: 12px; }
.sub { color: #667; font-size: 12px; margin-bottom: 14px; }
.bar { display: flex; gap: 10px; align-items: center; background: #fff; padding: 12px 14px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,.08); }
.bar label { font-size: 13px; white-space: nowrap; }
.bar input[type="text"], .bar input:not([type]) { flex: 1; padding: 7px 10px; border: 1px solid #ccd2d9; border-radius: 6px; font-size: 13px; }
.chk { display: flex; align-items: center; gap: 5px; }
.bar button { padding: 8px 22px; background: #2563eb; color: #fff; border: none; border-radius: 6px; font-size: 13px; cursor: pointer; }
.bar button:disabled { background: #93b4f5; cursor: wait; }
.bar button:hover:not(:disabled) { background: #1d4fd8; }
.err-bar { margin-top: 12px; padding: 10px 14px; background: #fdecea; color: #c0392b; border-radius: 6px; font-size: 13px; }
.hint { margin-top: 30px; text-align: center; color: #99a; font-size: 13px; }
.cards { display: flex; gap: 12px; margin-top: 16px; }
.card { flex: 1; background: #fff; border-radius: 8px; padding: 12px 8px; text-align: center; box-shadow: 0 1px 3px rgba(0,0,0,.08); }
.card b { display: block; font-size: 22px; }
.card span { font-size: 12px; color: #778; }
.card.bad b { color: #c0392b; }
.card.warn b { color: #b8860b; }
.card.ok b { color: #1e8449; }
.meta { font-size: 12px; color: #889; margin: 10px 2px; }
.block { margin-top: 18px; background: #fff; border-radius: 8px; padding: 14px 16px; box-shadow: 0 1px 3px rgba(0,0,0,.08); }
table { width: 100%; border-collapse: collapse; }
td, th { border: 1px solid #e3e6ea; padding: 6px 9px; font-size: 12.5px; text-align: left; vertical-align: top; }
th { background: #f0f2f5; }
.mono { font-family: Consolas, monospace; font-size: 12px; word-break: break-all; }
.small { font-size: 12px; }
.tech { list-style: none; }
.hd { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
.tools { display: flex; gap: 8px; align-items: center; flex-shrink: 0; padding-top: 2px; }
.ver { font-size: 12px; color: #889; }
button.ghost { padding: 6px 14px; background: #fff; color: #2563eb; border: 1px solid #b9cdf3; border-radius: 6px; font-size: 12.5px; cursor: pointer; }
button.ghost:hover { background: #eef4ff; }
button.primary { padding: 7px 18px; background: #2563eb; color: #fff; border: none; border-radius: 6px; font-size: 13px; cursor: pointer; }
button.primary:hover { background: #1d4fd8; }
button:disabled { opacity: .55; cursor: wait; }
.modal.left { text-align: left; width: 560px; }
.modal.left h3 { margin-bottom: 10px; }
.status-line { font-size: 12.5px; color: #667; margin: 8px 0 2px; }
.status-line .dirty { color: #b8860b; font-weight: 500; }
.status-line .clean { color: #1e8449; }
.log-panel { height: 170px; overflow-y: auto; background: #1e2430; border-radius: 8px; padding: 10px 12px; font-family: Consolas, monospace; font-size: 12px; line-height: 1.7; }
.log-line { color: #cdd6e4; white-space: pre-wrap; word-break: break-all; }
.log-time { color: #6b7a94; margin-right: 8px; }
.pub-hint { font-size: 12px; color: #778; background: #f2f6ff; border-radius: 6px; padding: 8px 10px; margin-bottom: 12px; line-height: 1.6; }
.f-label { display: block; font-size: 12.5px; color: #556; margin: 10px 0 4px; }
.f-input, .f-area { width: 100%; padding: 8px 10px; border: 1px solid #ccd2d9; border-radius: 6px; font-size: 13px; font-family: inherit; box-sizing: border-box; }
.f-area { resize: vertical; }
.row-btns { display: flex; gap: 10px; justify-content: flex-end; margin-top: 16px; }
.pub-msg { font-size: 12.5px; color: #1e8449; background: #eaf7ef; border-radius: 6px; padding: 8px 10px; margin-top: 10px; white-space: pre-wrap; }
.pub-err { font-size: 12.5px; color: #c0392b; background: #fdecea; border-radius: 6px; padding: 8px 10px; margin-top: 10px; white-space: pre-wrap; }
.upd-new { font-size: 14px; margin: 8px 0; }
.upd-ok { font-size: 14px; color: #1e8449; margin: 8px 0; }
.upd-err { font-size: 13px; color: #c0392b; margin: 8px 0; word-break: break-all; }
.modal-mask { position: fixed; inset: 0; background: rgba(30,40,60,.45); display: flex; align-items: center; justify-content: center; z-index: 50; }
.modal { background: #fff; border-radius: 12px; padding: 26px 34px; width: 460px; box-shadow: 0 8px 30px rgba(0,0,0,.18); text-align: center; }
.modal h3 { margin-bottom: 16px; font-size: 16px; }
.pbar { height: 10px; background: #e8ebf0; border-radius: 6px; overflow: hidden; }
.pfill { height: 100%; background: #2563eb; border-radius: 6px; transition: width .15s; }
.pname { margin-top: 14px; font-size: 13px; font-family: Consolas, monospace; color: #334; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.pnum { margin-top: 6px; font-size: 12px; color: #889; }
.tech li { padding: 5px 2px; font-size: 13px; border-bottom: 1px dashed #eef0f2; display: flex; gap: 8px; align-items: baseline; }
.tag { display: inline-block; padding: 1px 8px; border-radius: 10px; font-size: 11px; color: #fff; white-space: nowrap; }
.tag.error { background: #c0392b; }
.tag.warn { background: #b8860b; }
.tag.info { background: #7f8c8d; }
.tag.ok { background: #1e8449; }
.ok-text { color: #1e8449; font-size: 13px; }
</style>
