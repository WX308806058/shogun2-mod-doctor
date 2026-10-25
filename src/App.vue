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

    <header>
      <h1>幕府将军2 Mod 冲突检测器</h1>
      <p class="sub">按游戏真实加载顺序（boot → release → patch → mod → movie）解析 pack 内 db 表，检测同名表覆盖冲突与科技生态悬空引用（加载页闪退高发原因）</p>
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
        <h2>三、同名表覆盖冲突 <small>加载顺序靠后者覆盖靠前者</small></h2>
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
import type { ScanReport } from "./types";

const gameDir = ref("");
const includeAll = ref(false);
const scanning = ref(false);
const report = ref<ScanReport | null>(null);
const error = ref("");
const progress = ref({ current: 0, total: 0, name: "" });
const pct = computed(() =>
  progress.value.total ? Math.round((progress.value.current / progress.value.total) * 100) : 0
);

onMounted(() => {
  invoke<string>("default_game_dir").then((d) => { if (d) gameDir.value = d; }).catch(() => {});
  listen<{ current: number; total: number; name: string }>("scan-progress", (e) => {
    progress.value = e.payload;
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
