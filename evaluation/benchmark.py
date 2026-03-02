#!/usr/bin/env python3

import argparse
import csv
import json
import statistics
import time
import threading
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

import paho.mqtt.client as mqtt

# default broker is the tailscale ip of the mqtt container running
# alongside nebulastream in the ecs cluster
DEFAULT_BROKER = "10.0.10.241"
DEFAULT_PORT = 1883
DEFAULT_DURATION = 120
DEFAULT_NES_API = "http://nes-coordinator.iot.local:8081"

# topics published by the gateway's controller_app
RAW_TELEM_TOPIC = "controller_app/events"
NES_RESULTS_TOPIC = "nebulastream/results/#"
NES_ALERTS_TOPIC = "nebulastream/alerts/#"

OUTPUT_DIR = Path(__file__).parent / "results"

@dataclass
class MsgStats:
    # tracks per topic message counts, byte totals, and per second breakdown
    label: str
    msg_count: int = 0
    total_bytes: int = 0
    first_ts: float = 0.0
    last_ts: float = 0.0
    per_sec: list = field(default_factory=list)

@dataclass
class LatSample:
    # one latency measurement from telemetry timestamp to alert receipt
    telem_ts_ms: float
    alert_rx_ms: float
    lat_ms: float
    fld_name: str
    fld_value: float
    rule_cond: str

def _make_client(prefix: str, broker: str, port: int) -> mqtt.Client:
    c = mqtt.Client(
        client_id=f"{prefix}-{int(time.time())}",
        callback_api_version=mqtt.CallbackAPIVersion.VERSION2,
    )
    c.connect(broker, port, keepalive=60)
    return c

class BandwidthBenchmark:
    # measures raw telemetry vs nebulastream output bandwidth over the mqtt bus

    def __init__(self, broker: str, port: int, dur: int):
        self.broker = broker
        self.port = port
        self.dur = dur
        self.raw = MsgStats(label="raw_telemetry")
        self.nes_res = MsgStats(label="nes_results")
        self.nes_alrt = MsgStats(label="nes_alerts")
        self._lock = threading.Lock()
        self._t0 = 0.0
        self._cur_sec = 0
        self._raw_bps = 0
        self._nes_bps = 0
        self._alrt_bps = 0

    def _on_msg(self, _client, _userdata, msg):
        now = time.time()
        sz = len(msg.payload)
        elapsed = int(now - self._t0)

        with self._lock:
            # roll over per second counters when the clock ticks
            if elapsed > self._cur_sec:
                self.raw.per_sec.append(self._raw_bps)
                self.nes_res.per_sec.append(self._nes_bps)
                self.nes_alrt.per_sec.append(self._alrt_bps)
                for _ in range(elapsed - self._cur_sec - 1):
                    self.raw.per_sec.append(0)
                    self.nes_res.per_sec.append(0)
                    self.nes_alrt.per_sec.append(0)
                self._cur_sec = elapsed
                self._raw_bps = 0
                self._nes_bps = 0
                self._alrt_bps = 0

            if msg.topic == RAW_TELEM_TOPIC:
                st = self.raw
                self._raw_bps += sz
            elif msg.topic.startswith("nebulastream/alerts/"):
                st = self.nes_alrt
                self._alrt_bps += sz
            elif msg.topic.startswith("nebulastream/results/"):
                st = self.nes_res
                self._nes_bps += sz
            else:
                return

            st.msg_count += 1
            st.total_bytes += sz
            if st.first_ts == 0:
                st.first_ts = now
            st.last_ts = now

    def run(self) -> dict:
        print(f"[bandwidth] collecting data for {self.dur}s...")
        cli = _make_client("bench-bw", self.broker, self.port)
        cli.on_message = self._on_msg
        cli.subscribe(RAW_TELEM_TOPIC, qos=0)
        cli.subscribe(NES_RESULTS_TOPIC, qos=0)
        cli.subscribe(NES_ALERTS_TOPIC, qos=0)
        self._t0 = time.time()
        cli.loop_start()
        try:
            time.sleep(self.dur)
        except KeyboardInterrupt:
            print("\n[bandwidth] interrupted early, saving partial results.")
        cli.loop_stop()
        cli.disconnect()
        with self._lock:
            self.raw.per_sec.append(self._raw_bps)
            self.nes_res.per_sec.append(self._nes_bps)
            self.nes_alrt.per_sec.append(self._alrt_bps)
        return self._build()

    def _build(self) -> dict:
        out = {}
        for st in [self.raw, self.nes_res, self.nes_alrt]:
            d = st.last_ts - st.first_ts
            rate = st.msg_count / d if d > 0 else 0
            bps = st.total_bytes / d if d > 0 else 0
            out[st.label] = {
                "messages": st.msg_count, "total_bytes": st.total_bytes,
                "duration_sec": round(d, 2), "msg_per_sec": round(rate, 2),
                "bytes_per_sec": round(bps, 2), "kb_per_sec": round(bps / 1024, 3),
            }
        # bandwidth saving shows how much data the edge filtering removes
        raw_b = self.raw.total_bytes or 1
        filt_b = self.nes_alrt.total_bytes
        out["bandwidth_saving_pct"] = round((1 - filt_b / raw_b) * 100, 1)
        out["per_second"] = {
            "raw": self.raw.per_sec,
            "nes_results": self.nes_res.per_sec,
            "nes_alerts": self.nes_alrt.per_sec,
        }
        return out

class LatencyBenchmark:
    # measures end to end latency from the compressor register read
    # (timestamped in the gateway) to the nebulastream alert arriving on mqtt

    def __init__(self, broker: str, port: int, dur: int):
        self.broker = broker
        self.port = port
        self.dur = dur
        self.samples: list[LatSample] = []
        self._lock = threading.Lock()

    def _on_msg(self, _client, _userdata, msg):
        now_ms = time.time() * 1000
        try:
            raw = json.loads(msg.payload)
        except (json.JSONDecodeError, UnicodeDecodeError):
            return

        if not msg.topic.startswith("nebulastream/alerts/"):
            return

        rows = raw if isinstance(raw, list) else [raw]
        for row in rows:
            clean = {}
            for k, v in row.items():
                # nebulastream prefixes field names with source$, strip that
                key = k.split("$")[-1] if "$" in k else k
                clean[key] = v
            ts = clean.get("timestamp", 0)
            if ts == 0:
                continue
            lat = abs(now_ms - ts)
            fld_nm = "unknown"
            fld_val = 0.0
            skip = {"DEVICE_ID", "GATEWAY_ID", "timestamp"}
            for k, v in clean.items():
                if k not in skip and isinstance(v, (int, float)):
                    fld_nm = k
                    fld_val = float(v)
                    break
            s = LatSample(
                telem_ts_ms=ts, alert_rx_ms=now_ms,
                lat_ms=round(lat, 2), fld_name=fld_nm,
                fld_value=fld_val,
                rule_cond=msg.topic.split("/")[-1][:8],
            )
            with self._lock:
                self.samples.append(s)

    def run(self) -> dict:
        print(f"[latency] collecting latency samples for {self.dur}s...")
        cli = _make_client("bench-lat", self.broker, self.port)
        cli.on_message = self._on_msg
        cli.subscribe(NES_ALERTS_TOPIC, qos=0)
        cli.loop_start()
        try:
            time.sleep(self.dur)
        except KeyboardInterrupt:
            print("\n[latency] interrupted.")
        cli.loop_stop()
        cli.disconnect()
        return self._build()

    def _build(self) -> dict:
        if not self.samples:
            return {"sample_count": 0, "note": "no alert samples, is an alert rule active?"}
        lats = sorted(s.lat_ms for s in self.samples)
        return {
            "sample_count": len(lats),
            "min_ms": round(min(lats), 2),
            "max_ms": round(max(lats), 2),
            "mean_ms": round(statistics.mean(lats), 2),
            "median_ms": round(statistics.median(lats), 2),
            "stdev_ms": round(statistics.stdev(lats), 2) if len(lats) > 1 else 0,
            "p95_ms": round(lats[int(len(lats) * 0.95)], 2),
            "p99_ms": round(lats[int(len(lats) * 0.99)], 2),
            "samples": [
                {"telemetry_ts": s.telem_ts_ms, "alert_received": s.alert_rx_ms,
                 "latency_ms": s.lat_ms, "field": s.fld_name, "value": s.fld_value}
                for s in self.samples
            ],
        }

class EdgeVsCloudBenchmark:
    # compares latency of the "cloud path" (raw telemetry over mqtt) vs
    # the "edge path" (nebulastream alert after on device filtering)

    def __init__(self, broker: str, port: int, dur: int):
        self.broker = broker
        self.port = port
        self.dur = dur
        self._lock = threading.Lock()
        self.raw_lats: list[float] = []
        self.alert_lats: list[float] = []

    def _on_msg(self, _client, _userdata, msg):
        now_ms = time.time() * 1000
        try:
            raw = json.loads(msg.payload)
        except (json.JSONDecodeError, UnicodeDecodeError):
            return

        if msg.topic == RAW_TELEM_TOPIC:
            ts = raw.get("timestamp", 0)
            if ts > 0:
                with self._lock:
                    self.raw_lats.append(now_ms - ts)

        elif msg.topic.startswith("nebulastream/alerts/"):
            rows = raw if isinstance(raw, list) else [raw]
            for row in rows:
                clean = {(k.split("$")[-1] if "$" in k else k): v for k, v in row.items()}
                ts = clean.get("timestamp", 0)
                if ts > 0:
                    with self._lock:
                        self.alert_lats.append(now_ms - ts)

    def run(self) -> dict:
        print(f"[edge vs cloud] collecting for {self.dur}s...")
        cli = _make_client("bench-evc", self.broker, self.port)
        cli.on_message = self._on_msg
        cli.subscribe(RAW_TELEM_TOPIC, qos=0)
        cli.subscribe(NES_ALERTS_TOPIC, qos=0)
        cli.loop_start()
        try:
            time.sleep(self.dur)
        except KeyboardInterrupt:
            print("\n[edge vs cloud] interrupted.")
        cli.loop_stop()
        cli.disconnect()
        return self._build()

    def _build(self) -> dict:
        def calc(data: list[float]) -> dict:
            if not data:
                return {"count": 0, "min": 0, "max": 0, "mean": 0, "median": 0, "stdev": 0}
            return {
                "count": len(data), "min": round(min(data), 2),
                "max": round(max(data), 2), "mean": round(statistics.mean(data), 2),
                "median": round(statistics.median(data), 2),
                "stdev": round(statistics.stdev(data), 2) if len(data) > 1 else 0,
            }
        cl = calc(self.raw_lats)
        ed = calc(self.alert_lats)
        overhead = round(ed["mean"] - cl["mean"], 2) if ed["count"] > 0 and cl["count"] > 0 else 0
        return {
            "cloud_path": cl, "edge_path": ed,
            "edge_overhead_ms": overhead,
            "raw_cloud_samples": self.raw_lats,
            "raw_edge_samples": self.alert_lats,
        }

class MessageSizeBenchmark:
    # analyzes payload sizes to quantify how much smaller nebulastream alerts
    # are compared to the raw modbus telemetry json from the compressor

    def __init__(self, broker: str, port: int, dur: int):
        self.broker = broker
        self.port = port
        self.dur = dur
        self._lock = threading.Lock()
        self.raw_sz: list[int] = []
        self.alrt_sz: list[int] = []
        self.res_sz: list[int] = []
        self.raw_fld_cnt: list[int] = []
        self.alrt_fld_cnt: list[int] = []

    def _on_msg(self, _client, _userdata, msg):
        sz = len(msg.payload)
        try:
            payload = json.loads(msg.payload)
        except (json.JSONDecodeError, UnicodeDecodeError):
            return

        with self._lock:
            if msg.topic == RAW_TELEM_TOPIC:
                self.raw_sz.append(sz)
                if isinstance(payload, dict):
                    self.raw_fld_cnt.append(len(payload))
            elif msg.topic.startswith("nebulastream/alerts/"):
                self.alrt_sz.append(sz)
                rows = payload if isinstance(payload, list) else [payload]
                if rows:
                    self.alrt_fld_cnt.append(len(rows[0]))
            elif msg.topic.startswith("nebulastream/results/"):
                self.res_sz.append(sz)

    def run(self) -> dict:
        print(f"[message size] collecting for {self.dur}s...")
        cli = _make_client("bench-msz", self.broker, self.port)
        cli.on_message = self._on_msg
        cli.subscribe(RAW_TELEM_TOPIC, qos=0)
        cli.subscribe(NES_ALERTS_TOPIC, qos=0)
        cli.subscribe(NES_RESULTS_TOPIC, qos=0)
        cli.loop_start()
        try:
            time.sleep(self.dur)
        except KeyboardInterrupt:
            print("\n[message size] interrupted.")
        cli.loop_stop()
        cli.disconnect()
        return self._build()

    def _build(self) -> dict:
        def sz_stats(sizes: list[int]) -> dict:
            if not sizes:
                return {"count": 0, "min": 0, "max": 0, "mean": 0, "median": 0, "total": 0}
            return {
                "count": len(sizes), "min": min(sizes), "max": max(sizes),
                "mean": round(statistics.mean(sizes), 1),
                "median": round(statistics.median(sizes), 1),
                "total": sum(sizes),
            }
        raw = sz_stats(self.raw_sz)
        alrt = sz_stats(self.alrt_sz)
        res = sz_stats(self.res_sz)
        reduction = round((1 - alrt["mean"] / raw["mean"]) * 100, 1) if raw["mean"] > 0 and alrt["mean"] > 0 else 0
        avg_raw_f = round(statistics.mean(self.raw_fld_cnt), 1) if self.raw_fld_cnt else 0
        avg_alrt_f = round(statistics.mean(self.alrt_fld_cnt), 1) if self.alrt_fld_cnt else 0
        return {
            "raw_telemetry": raw, "nes_alerts": alrt, "nes_results": res,
            "per_message_reduction_pct": reduction,
            "avg_raw_fields": avg_raw_f, "avg_alert_fields": avg_alrt_f,
            "raw_sizes": self.raw_sz, "alert_sizes": self.alrt_sz,
        }

class QueryDeployBenchmark:
    # measures time from query submission to first result arriving on mqtt,
    # this tells us how fast nebulastream can spin up a new stream processing query

    def __init__(self, broker: str, port: int, nes_api: str):
        self.broker = broker
        self.port = port
        self.nes_api = nes_api
        self.samples: list[dict] = []

    def _deploy_and_measure(self, src: str, sink_url: str, trial: int) -> dict:
        import requests
        import uuid

        rid = str(uuid.uuid4())
        topic = f"nebulastream/results/{rid}"

        # TODO: make the dsl configurable per device type
        dsl = (
            f'Query::from("{src}")'
            f'.sink(MQTTSinkDescriptor::create("{sink_url}", "{topic}", "", '
            f'1000, MQTTSinkDescriptor::TimeUnits::milliseconds, 1));'
        )

        first_result = [None]
        evt = threading.Event()

        def on_msg(_client, _userdata, msg):
            if msg.topic == topic and first_result[0] is None:
                first_result[0] = time.time()
                evt.set()

        cli = _make_client(f"bench-qd-{trial}", self.broker, self.port)
        cli.on_message = on_msg
        cli.subscribe(topic, qos=0)
        cli.loop_start()

        t_submit = time.time()
        try:
            resp = requests.post(
                f"{self.nes_api}/v1/nes/query/execute-query",
                json={"userQuery": dsl, "placement": "BottomUp"},
                timeout=10,
            )
            if not resp.ok:
                cli.loop_stop()
                cli.disconnect()
                return {"trial": trial, "error": f"HTTP {resp.status_code}: {resp.text[:200]}"}
            qid = resp.json().get("queryId")
        except Exception as e:
            cli.loop_stop()
            cli.disconnect()
            return {"trial": trial, "error": str(e)}

        evt.wait(timeout=30)
        cli.loop_stop()
        cli.disconnect()

        # clean up the query so we don't leak resources on the coordinator
        try:
            requests.delete(
                f"{self.nes_api}/v1/nes/query/stop-query",
                json={"queryId": qid}, timeout=10,
            )
        except Exception:
            pass

        if first_result[0] is None:
            return {"trial": trial, "error": "timeout, no result in 30s", "query_id": qid}

        deploy_lat = round((first_result[0] - t_submit) * 1000, 2)
        return {"trial": trial, "deploy_latency_ms": deploy_lat, "query_id": qid}

    def run(self, source: str = "telemetry_0x0007", trials: int = 5) -> dict:
        print(f"[query deploy] running {trials} deployment trials...")
        sink_url = f"tcp://{self.broker}:{self.port}"

        results = []
        for i in range(trials):
            print(f"  trial {i+1}/{trials}...")
            r = self._deploy_and_measure(source, sink_url, i + 1)
            results.append(r)
            print(f"    -> {r.get('deploy_latency_ms', r.get('error', '?'))} ms")
            # nebulastream needs a breather between successive deploy/stop cycles,
            # without this the coordinator sometimes rejects the next query
            time.sleep(5)

        lats = [r["deploy_latency_ms"] for r in results if "deploy_latency_ms" in r]
        summary = {}
        if lats:
            summary = {
                "successful_trials": len(lats),
                "failed_trials": len(results) - len(lats),
                "min_ms": round(min(lats), 2),
                "max_ms": round(max(lats), 2),
                "mean_ms": round(statistics.mean(lats), 2),
                "median_ms": round(statistics.median(lats), 2),
            }
        else:
            summary = {"successful_trials": 0, "failed_trials": len(results)}
        summary["trials"] = results
        return summary

class ScalabilityBenchmark:
    # counts how many concurrent nebulastream queries are running and measures
    # aggregate throughput, helps evaluate the arm64 gateway's processing limits

    def __init__(self, broker: str, port: int, dur: int):
        self.broker = broker
        self.port = port
        self.dur = dur

    def run(self) -> dict:
        print(f"[scalability] measuring concurrent query throughput for {self.dur}s...")

        topic_st: dict[str, dict] = {}
        lock = threading.Lock()
        t0 = time.time()

        def on_msg(_client, _userdata, msg):
            now = time.time()
            sz = len(msg.payload)
            with lock:
                if msg.topic not in topic_st:
                    topic_st[msg.topic] = {"count": 0, "bytes": 0, "first": now, "last": now}
                s = topic_st[msg.topic]
                s["count"] += 1
                s["bytes"] += sz
                s["last"] = now

        cli = _make_client("bench-scl", self.broker, self.port)
        cli.on_message = on_msg
        cli.subscribe(RAW_TELEM_TOPIC, qos=0)
        cli.subscribe(NES_RESULTS_TOPIC, qos=0)
        cli.subscribe(NES_ALERTS_TOPIC, qos=0)
        cli.loop_start()
        try:
            time.sleep(self.dur)
        except KeyboardInterrupt:
            print("\n[scalability] interrupted.")
        cli.loop_stop()
        cli.disconnect()

        elapsed = time.time() - t0
        nes_topics = {t: s for t, s in topic_st.items() if t.startswith("nebulastream/")}
        raw_t = topic_st.get(RAW_TELEM_TOPIC, {"count": 0, "bytes": 0})

        per_q = []
        for topic, s in sorted(nes_topics.items()):
            d = s["last"] - s["first"] if s["count"] > 1 else elapsed
            per_q.append({
                "topic": topic.split("/")[-1][:12],
                "type": "alert" if "/alerts/" in topic else "result",
                "messages": s["count"], "bytes": s["bytes"],
                "msg_per_sec": round(s["count"] / d, 2) if d > 0 else 0,
                "kb_per_sec": round(s["bytes"] / d / 1024, 3) if d > 0 else 0,
            })

        tot_msgs = sum(s["count"] for s in nes_topics.values())
        tot_bytes = sum(s["bytes"] for s in nes_topics.values())

        return {
            "duration_sec": round(elapsed, 1),
            "concurrent_queries": len(nes_topics),
            "raw_telemetry_messages": raw_t["count"],
            "raw_telemetry_bytes": raw_t["bytes"],
            "total_nes_messages": tot_msgs, "total_nes_bytes": tot_bytes,
            "total_nes_msg_per_sec": round(tot_msgs / elapsed, 2) if elapsed > 0 else 0,
            "total_nes_kb_per_sec": round(tot_bytes / elapsed / 1024, 3) if elapsed > 0 else 0,
            "per_query": per_q,
        }

def save_bandwidth_csv(results: dict, out_dir: Path):
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / "bandwidth_summary.csv"
    with open(p, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["category", "messages", "total_bytes", "duration_sec", "msg_per_sec", "bytes_per_sec", "kb_per_sec"])
        for lbl in ["raw_telemetry", "nes_results", "nes_alerts"]:
            d = results[lbl]
            w.writerow([lbl, d["messages"], d["total_bytes"], d["duration_sec"], d["msg_per_sec"], d["bytes_per_sec"], d["kb_per_sec"]])
    print(f"  -> {p}")

    # per second timeseries for plotting bandwidth over time
    ts_p = out_dir / "bandwidth_timeseries.csv"
    ps = results["per_second"]
    mx = max(len(ps["raw"]), len(ps["nes_results"]), len(ps["nes_alerts"]))
    with open(ts_p, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["second", "raw_bytes", "nes_results_bytes", "nes_alerts_bytes"])
        for i in range(mx):
            w.writerow([i,
                        ps["raw"][i] if i < len(ps["raw"]) else 0,
                        ps["nes_results"][i] if i < len(ps["nes_results"]) else 0,
                        ps["nes_alerts"][i] if i < len(ps["nes_alerts"]) else 0])
    print(f"  -> {ts_p}")

def save_latency_csv(results: dict, out_dir: Path):
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / "latency_summary.csv"
    with open(p, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["metric", "value_ms"])
        for key in ["min_ms", "max_ms", "mean_ms", "median_ms", "stdev_ms", "p95_ms", "p99_ms"]:
            if key in results:
                w.writerow([key.replace("_ms", ""), results[key]])
    print(f"  -> {p}")
    if "samples" in results and results["samples"]:
        sp = out_dir / "latency_samples.csv"
        with open(sp, "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["telemetry_ts", "alert_received", "latency_ms", "field", "value"])
            for s in results["samples"]:
                w.writerow([s["telemetry_ts"], s["alert_received"], s["latency_ms"], s["field"], s["value"]])
        print(f"  -> {sp}")

def save_edge_vs_cloud_csv(results: dict, out_dir: Path):
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / "edge_vs_cloud_summary.csv"
    with open(p, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["path", "count", "min_ms", "max_ms", "mean_ms", "median_ms", "stdev_ms"])
        for lbl, key in [("cloud", "cloud_path"), ("edge", "edge_path")]:
            d = results[key]
            w.writerow([lbl, d["count"], d["min"], d["max"], d["mean"], d["median"], d["stdev"]])
    print(f"  -> {p}")
    sp = out_dir / "edge_vs_cloud_samples.csv"
    with open(sp, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["path", "latency_ms"])
        for v in results.get("raw_cloud_samples", []):
            w.writerow(["cloud", round(v, 2)])
        for v in results.get("raw_edge_samples", []):
            w.writerow(["edge", round(v, 2)])
    print(f"  -> {sp}")

def save_message_size_csv(results: dict, out_dir: Path):
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / "message_size_summary.csv"
    with open(p, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["category", "count", "min_bytes", "max_bytes", "mean_bytes", "median_bytes"])
        for lbl in ["raw_telemetry", "nes_alerts", "nes_results"]:
            d = results[lbl]
            w.writerow([lbl, d["count"], d["min"], d["max"], d["mean"], d["median"]])
    print(f"  -> {p}")

def save_query_deploy_csv(results: dict, out_dir: Path):
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / "query_deploy_summary.csv"
    with open(p, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["trial", "deploy_latency_ms", "error"])
        for t in results.get("trials", []):
            w.writerow([t["trial"], t.get("deploy_latency_ms", ""), t.get("error", "")])
    print(f"  -> {p}")

def save_scalability_csv(results: dict, out_dir: Path):
    out_dir.mkdir(parents=True, exist_ok=True)
    p = out_dir / "scalability_summary.csv"
    with open(p, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["metric", "value"])
        w.writerow(["concurrent_queries", results["concurrent_queries"]])
        w.writerow(["total_nes_messages", results["total_nes_messages"]])
        w.writerow(["total_nes_bytes", results["total_nes_bytes"]])
        w.writerow(["total_nes_msg_per_sec", results["total_nes_msg_per_sec"]])
        w.writerow(["total_nes_kb_per_sec", results["total_nes_kb_per_sec"]])
    print(f"  -> {p}")
    pq = out_dir / "scalability_per_query.csv"
    with open(pq, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["query_id", "type", "messages", "bytes", "msg_per_sec", "kb_per_sec"])
        for q in results.get("per_query", []):
            w.writerow([q["topic"], q["type"], q["messages"], q["bytes"], q["msg_per_sec"], q["kb_per_sec"]])
    print(f"  -> {pq}")

def _print_results(title: str, results: dict):
    print(f"\n{'=' * 65}")
    print(f"  {title}")
    print("=" * 65)
    _print_dict(results, indent=2)
    print("=" * 65)

def _print_dict(d: dict, indent: int = 0):
    pfx = " " * indent
    skip_keys = {"samples", "per_second", "per_query", "trials",
                 "raw_cloud_samples", "raw_edge_samples", "raw_sizes", "alert_sizes"}
    for k, v in d.items():
        if k in skip_keys:
            if isinstance(v, list):
                print(f"{pfx}{k}: [{len(v)} items]")
            continue
        if isinstance(v, dict):
            print(f"{pfx}{k}:")
            _print_dict(v, indent + 4)
        else:
            print(f"{pfx}{k}: {v}")

ALL_MODES = ["bandwidth", "latency", "edge-vs-cloud", "message-size", "query-deploy", "scalability"]

def main():
    parser = argparse.ArgumentParser(
        description="Aura edge computing platform, evaluation benchmarks for IoT.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("mode", choices=ALL_MODES + ["all"], help="which benchmark to run.")
    parser.add_argument("--broker", default=DEFAULT_BROKER, help=f"mqtt broker (default: {DEFAULT_BROKER})")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help=f"mqtt port (default: {DEFAULT_PORT})")
    parser.add_argument("--duration", type=int, default=DEFAULT_DURATION, help=f"duration in seconds (default: {DEFAULT_DURATION})")
    parser.add_argument("--nes-api", default=DEFAULT_NES_API, help=f"nebulastream coordinator REST API (default: {DEFAULT_NES_API})")
    parser.add_argument("--output", default=str(OUTPUT_DIR), help=f"output directory (default: {OUTPUT_DIR})")

    args = parser.parse_args()
    out_dir = Path(args.output)
    run_dir = out_dir / datetime.now().strftime("%Y%m%d_%H%M%S")
    modes = ALL_MODES if args.mode == "all" else [args.mode]

    print(f"\nAura Edge Computing Platform, Evaluation Benchmark Suite")
    print(f"Broker: {args.broker}:{args.port}")
    print(f"Duration: {args.duration}s | Benchmarks: {', '.join(modes)}")
    print(f"Output: {run_dir}\n")

    if "bandwidth" in modes:
        bw = BandwidthBenchmark(args.broker, args.port, args.duration)
        r = bw.run()
        _print_results("BANDWIDTH BENCHMARK", r)
        save_bandwidth_csv(r, run_dir)

    if "latency" in modes:
        lat = LatencyBenchmark(args.broker, args.port, args.duration)
        r = lat.run()
        _print_results("ALERT LATENCY BENCHMARK", r)
        save_latency_csv(r, run_dir)

    if "edge-vs-cloud" in modes:
        evc = EdgeVsCloudBenchmark(args.broker, args.port, min(args.duration, 60))
        r = evc.run()
        _print_results("EDGE vs CLOUD LATENCY", r)
        save_edge_vs_cloud_csv(r, run_dir)

    if "message-size" in modes:
        msz = MessageSizeBenchmark(args.broker, args.port, min(args.duration, 60))
        r = msz.run()
        _print_results("MESSAGE SIZE ANALYSIS", r)
        save_message_size_csv(r, run_dir)

    if "query-deploy" in modes:
        qd = QueryDeployBenchmark(args.broker, args.port, args.nes_api)
        r = qd.run(trials=5)
        _print_results("QUERY DEPLOYMENT LATENCY", r)
        save_query_deploy_csv(r, run_dir)

    if "scalability" in modes:
        scl = ScalabilityBenchmark(args.broker, args.port, min(args.duration, 60))
        r = scl.run()
        _print_results("CONCURRENT QUERY SCALABILITY", r)
        save_scalability_csv(r, run_dir)

    print(f"\nall results saved to {run_dir}/")

if __name__ == "__main__":
    main()
