"""Generate the hashistack-rdw dashboards.

The JSON files one directory up are this script's output; edit here, not there:

    python3 examples/demo/hashistack-rdw/generator/generate.py [OUT_DIR]

OUT_DIR defaults to examples/demo/hashistack-rdw/. Then re-validate:
    cargo test --test validate_cli validate_strict_accepts_every_hashistack_rdw_dashboard

Metric and label names were checked against a live Prometheus 3 scraping Consul,
Nomad, Vault, node_exporter and Consul Connect Envoy sidecars (see ../README.md).
"""
import os
import sys

from dash import (AMBER, BLUE, GREEN, NEUTRAL, ONE_GOOD, RED, UTIL, ZERO_GOOD, ZERO_GOOD_AMBER, bargauge,
                  dashboard, empty_ok, query_var, stat, steps, table, ts, write)

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.makedirs(OUT, exist_ok=True)
RI = "$__rate_interval"
LAT = "5m"  # histogram window: inbound HTTP on this mesh is sparse, 1m windows are mostly empty

# Thresholds that mirror PLAN-Prometheus F4 alert rules.
AGENT_CERT = steps(RED, (4 * 3600, AMBER), (8 * 3600, GREEN))           # *CertExpiringSoon: < 4h; vault-agent renews in a 10-26h sawtooth
LEAF_CERT = steps(RED, (2 * 3600, AMBER), (6 * 3600, GREEN))            # Connect leaves rotate with ~7-29h left
LEAF_EXPIRY = "envoy_listener_ssl_certificate_expiration_unix_time_seconds"
CA_CERT = steps(RED, (30 * 86400, AMBER), (90 * 86400, GREEN))         # *CAExpiringSoon: < 30d
DISK = steps(GREEN, (0.8, AMBER), (0.9, RED))                          # HostDiskFilling: < 10% free
RATIO_ERR = steps(GREEN, (0.01, AMBER), (0.05, RED))
EXACTLY_ONE = steps(RED, (1, GREEN), (2, RED))
TOLERANCE = steps(AMBER, (1, GREEN))                                   # 0 = no server may fail

ROOT_FS = 'mountpoint="/",fstype=~"xfs|ext4"'
PUB = 'envoy_http_conn_manager_prefix="public_listener"'
UPSTREAMS = 'envoy_cluster_name!~"local_app|local_agent|self_admin"'

# ---------------------------------------------------------------- 00 overview
overview = dashboard(
    "hashistack-overview", "HashiStack / Fleet Overview",
    "Is gcloud-dc healthy right now? Alert state, control-plane quorum, certificate runway, capacity "
    "headroom and mesh error rate on one screen. Drill into the per-subsystem dashboards from here.",
    ["overview"],
    [
        ("Health -- every tile should be green", [
            stat("Firing alerts", 'count(ALERTS{alertstate="firing",alertname!="Watchdog"}) or vector(0)',
                 th=ZERO_GOOD, desc="Alerts from hashistack.rules.yml currently firing (Watchdog excluded). "
                 "There is no Alertmanager: read the alert names in the table below.", graph=False),
            stat("Pending alerts", 'count(ALERTS{alertstate="pending"}) or vector(0)', th=ZERO_GOOD_AMBER,
                 desc="Rules whose expression is true but whose `for:` has not yet elapsed.", graph=False),
            stat("Targets down", 'count(up == 0) or vector(0)', th=ZERO_GOOD,
                 desc="Scrape targets Prometheus cannot reach (alert TargetDown after 5m)."),
            stat("Vault sealed", 'count(vault_core_unsealed == 0) or vector(0)', th=ZERO_GOOD,
                 desc="Vault servers reporting sealed (alert VaultSealed after 1m).", graph=False),
            stat("Raft quorums unhealthy",
                 '(count(consul_autopilot_healthy == 0) or vector(0)) + (count(nomad_nomad_autopilot_healthy == 0) '
                 'or vector(0)) + (count(vault_autopilot_healthy == 0) or vector(0))', th=ZERO_GOOD,
                 desc="Consul, Nomad and Vault autopilot clusters reporting unhealthy.", graph=False),
            stat("Failed systemd units", 'count(node_systemd_unit_state{state="failed"} == 1) or vector(0)',
                 th=ZERO_GOOD, desc="Platform units (consul, nomad, vault, vault-agent@*, node_exporter, "
                 "podman-image-gc) in state failed on any host (alert PlatformUnitFailed).", graph=False),
            stat("Shortest agent TLS cert",
                 'min({__name__=~"consul_agent_tls_cert_expiry|nomad_agent_tls_cert_expiration_seconds"})',
                 unit="s", th=AGENT_CERT, desc="Seconds until the soonest Consul or Nomad agent certificate "
                 "expires. vault-agent renews them; red below 4h is the alert threshold."),
            stat("Shortest CA",
                 'min({__name__=~"consul_mesh_active_root_ca_expiry|consul_mesh_active_signing_ca_expiry|'
                 'nomad_agent_tls_ca_expiration_seconds"})', unit="s", th=CA_CERT,
                 desc="Runway of the Connect root/signing CA and the agent CA. Red below 30 days."),
            stat("Shortest mesh leaf cert", f'min({LEAF_EXPIRY} > 0) - time()', unit="s", th=LEAF_CERT,
                 desc="Seconds until the soonest inbound mTLS leaf certificate on any sidecar expires. Connect "
                 "re-signs leaves with roughly 7-29h left; below 2h rotation has stalled."),
            stat("Envoy sidecars live", 'sum(envoy_server_live)', th=ONE_GOOD,
                 desc="Scraped sidecars reporting LIVE. Compare with the targets-by-job panel."),
            stat("Running allocations", 'sum(nomad_client_allocations_running)',
                 desc="Allocations running across every Nomad client."),
            stat("Mesh 5xx ratio",
                 f'(sum(rate(envoy_http_downstream_rq_xx{{{PUB},envoy_response_code_class="5"}}[{RI}])) or vector(0))'
                 f' / clamp_min(sum(rate(envoy_http_downstream_rq_total{{{PUB}}}[{RI}])), 1e-9)',
                 unit="percentunit", th=RATIO_ERR, decimals=2,
                 desc="Share of inbound HTTP requests answered 5xx across every sidecar's public listener."),
        ]),
        ("Alerts and scrape targets", [
            empty_ok(table("Active alerts", [('ALERTS', '{{alertname}} ({{alertstate}}, {{severity}}) {{instance}}')],
                  th=steps(BLUE), w=12, h=5,
                  desc="Every pending or firing alert. Watchdog always fires: it proves the rule engine runs.")),
            bargauge("Target availability by job", [('avg by (job) (up)', '{{job}}')], unit="percentunit",
                     th=steps(RED, (0.999, GREEN)), w=12, h=5, mx=1,
                     desc="Fraction of each job's targets answering their last scrape."),
        ]),
        ("Capacity headroom", [
            ts("Host CPU busy", [(f'1 - avg by (instance) (rate(node_cpu_seconds_total{{mode="idle"}}[{RI}]))',
                                  '{{instance}}')], unit="percentunit", mx=1, th=UTIL, th_style="dashed",
               w=8, desc="Non-idle CPU per host (node_exporter)."),
            ts("Host memory used", [('1 - node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes',
                                     '{{instance}}')], unit="percentunit", mx=1, th=UTIL, th_style="dashed",
               w=8, desc="1 - MemAvailable/MemTotal per host."),
            ts("Root filesystem used", [(f'1 - node_filesystem_avail_bytes{{{ROOT_FS}}} / '
                                         f'node_filesystem_size_bytes{{{ROOT_FS}}}', '{{instance}}')],
               unit="percentunit", mx=1, th=DISK, th_style="dashed", w=8,
               desc="Alert HostDiskFilling fires below 10% free (red line)."),
            ts("Nomad CPU reserved", [('nomad_client_allocated_cpu / (nomad_client_allocated_cpu + '
                                       'nomad_client_unallocated_cpu)', '{{instance}} ({{node_pool}})')],
               unit="percentunit", mx=1, th=UTIL, th_style="dashed", w=12,
               desc="Share of each client's schedulable CPU (MHz) reserved by allocations. Placement fails at 100%."),
            ts("Nomad memory reserved", [('nomad_client_allocated_memory / (nomad_client_allocated_memory + '
                                          'nomad_client_unallocated_memory)', '{{instance}} ({{node_pool}})')],
               unit="percentunit", mx=1, th=UTIL, th_style="dashed", w=12,
               desc="Share of each client's schedulable memory reserved by allocations (memory_max not counted)."),
        ]),
        ("Workloads and mesh traffic", [
            ts("Nomad allocations by state", [
                ('sum(nomad_client_allocations_running)', 'running'),
                ('sum(nomad_client_allocations_pending)', 'pending'),
                ('sum(nomad_client_allocations_blocked)', 'blocked'),
                ('sum(nomad_client_allocations_migrating)', 'migrating'),
                ('sum(nomad_nomad_job_summary_failed)', 'failed (job summaries)'),
                ('sum(nomad_nomad_job_summary_lost)', 'lost (job summaries)')], w=12,
               desc="Client-side allocation states plus failed/lost counts from the servers' job summaries."),
            ts("Mesh inbound HTTP by status class", [
                (f'sum by (envoy_response_code_class) (rate(envoy_http_downstream_rq_xx{{{PUB}}}[{RI}]))',
                 '{{envoy_response_code_class}}xx')], unit="reqps", w=12, stack=True, fill=30,
               desc="Requests reaching every sidecar's public listener, by response class."),
        ]),
    ])

# ---------------------------------------------------------------- 10 nodes
I = 'instance=~"$instance"'
CPU_IDLE = f'rate(node_cpu_seconds_total{{mode="idle",{I}}}[{RI}])'
nodes = dashboard(
    "hashistack-nodes", "HashiStack / Nodes",
    "Host-level saturation and errors from node_exporter on every Consul, Nomad and Vault machine: "
    "CPU, memory, disk, network, clock, SELinux and the platform systemd units.",
    ["nodes", "node_exporter"],
    [
        ("Summary for the selected hosts", [
            stat("Hosts up", f'count(up{{job="node",{I}}} == 1)', th=ONE_GOOD, desc="node_exporter targets answering."),
            stat("Hottest CPU", f'max(1 - avg by (instance) ({CPU_IDLE}))', unit="percentunit", th=UTIL,
                 decimals=0, desc="Busiest selected host's non-idle CPU."),
            stat("Fullest memory", f'max(1 - node_memory_MemAvailable_bytes{{{I}}} / node_memory_MemTotal_bytes{{{I}}})',
                 unit="percentunit", th=UTIL, decimals=0, desc="Highest memory use among the selected hosts."),
            stat("Fullest root FS", f'max(1 - node_filesystem_avail_bytes{{{ROOT_FS},{I}}} / '
                 f'node_filesystem_size_bytes{{{ROOT_FS},{I}}})', unit="percentunit", th=DISK, decimals=0,
                 desc="Highest root filesystem use. HostDiskFilling fires above 90%."),
            stat("Peak load / core", f'max(node_load5{{{I}}} / on (instance) count by (instance) '
                 f'(node_cpu_seconds_total{{mode="idle",{I}}}))', th=steps(GREEN, (1, AMBER), (2, RED)), decimals=2,
                 desc="5-minute load average divided by CPU count; above 1 means runnable work is queueing."),
            stat("OOM kills (range)", f'sum(increase(node_vmstat_oom_kill{{{I}}}[$__range])) or vector(0)',
                 th=ZERO_GOOD, decimals=0, desc="Kernel OOM-killer invocations in the dashboard time range.",
                 graph=False),
            stat("Platform units not active", f'count(node_systemd_unit_state{{state="active",{I},'
                 f'name!="podman-image-gc.service"}} == 0) or vector(0)', th=ZERO_GOOD, graph=False,
                 desc="consul, nomad, vault, vault-agent@*, node_exporter, podman-image-gc.timer units that are "
                 "not active (the image-gc oneshot service is expected to be inactive between timer runs)."),
            stat("Clocks unsynchronised", f'count(node_timex_sync_status{{{I}}} == 0) or vector(0)', th=ZERO_GOOD,
                 graph=False, desc="Hosts whose kernel reports the clock unsynchronised. Raft, TLS and JWT "
                 "validation all assume synchronised clocks."),
            stat("Not SELinux enforcing", f'count(node_selinux_current_mode{{{I}}} != 1) or vector(0)',
                 th=ZERO_GOOD, graph=False, desc="Hosts not in SELinux enforcing mode (ADR-0134 relies on container_t)."),
            stat("Conntrack peak use", f'max(node_nf_conntrack_entries{{{I}}} / node_nf_conntrack_entries_limit{{{I}}})',
                 unit="percentunit", th=UTIL, decimals=1,
                 desc="Fullest connection-tracking table. A full table silently drops new connections."),
            stat("TCP retransmit ratio", f'sum(rate(node_netstat_Tcp_RetransSegs{{{I}}}[{RI}])) / '
                 f'clamp_min(sum(rate(node_netstat_Tcp_OutSegs{{{I}}}[{RI}])), 1e-9)', unit="percentunit",
                 th=RATIO_ERR, decimals=2, desc="Retransmitted / sent TCP segments over the selected hosts."),
            stat("NIC errors + drops /s", f'sum(rate(node_network_receive_errs_total{{device="eth0",{I}}}[{RI}]) + '
                 f'rate(node_network_transmit_errs_total{{device="eth0",{I}}}[{RI}]) + '
                 f'rate(node_network_receive_drop_total{{device="eth0",{I}}}[{RI}]) + '
                 f'rate(node_network_transmit_drop_total{{device="eth0",{I}}}[{RI}]))', th=ZERO_GOOD_AMBER,
                 decimals=2, desc="eth0 receive/transmit errors and drops per second."),
        ]),
        ("Ranking (now)", [
            bargauge("CPU busy by host", [(f'1 - avg by (instance) ({CPU_IDLE})', '{{instance}}')],
                     unit="percentunit", mx=1, desc="Current non-idle CPU per host."),
            bargauge("Memory used by host", [(f'1 - node_memory_MemAvailable_bytes{{{I}}} / '
                                              f'node_memory_MemTotal_bytes{{{I}}}', '{{instance}}')],
                     unit="percentunit", mx=1, desc="Current memory use per host."),
            bargauge("Root FS used by host", [(f'1 - node_filesystem_avail_bytes{{{ROOT_FS},{I}}} / '
                                               f'node_filesystem_size_bytes{{{ROOT_FS},{I}}}', '{{instance}}')],
                     unit="percentunit", mx=1, th=DISK, desc="Current root filesystem use per host."),
        ]),
        ("CPU", [
            ts("CPU busy", [(f'1 - avg by (instance) ({CPU_IDLE})', '{{instance}}')], unit="percentunit", mx=1,
               th=UTIL, th_style="dashed", w=12, desc="Non-idle CPU per host."),
            ts("CPU by mode (selected hosts)", [
                (f'sum by (mode) (rate(node_cpu_seconds_total{{mode!="idle",{I}}}[{RI}])) / '
                 f'scalar(count(node_cpu_seconds_total{{mode="idle",{I}}}))', '{{mode}}')],
               unit="percentunit", stack=True, fill=40, w=12,
               desc="Where the busy time goes. High iowait points at disk; high steal at the hypervisor."),
            ts("Load average per core", [(f'node_load5{{{I}}} / on (instance) count by (instance) '
                                          f'(node_cpu_seconds_total{{mode="idle",{I}}})', '{{instance}}')],
               th=steps(GREEN, (1, AMBER), (2, RED)), th_style="dashed", decimals=2, w=8,
               desc="load5 / cores. Sustained values above 1 mean CPU queueing."),
            ts("iowait + steal", [(f'avg by (instance) (rate(node_cpu_seconds_total{{mode=~"iowait|steal",{I}}}[{RI}]))',
                                   '{{instance}}')], unit="percentunit", w=8,
               desc="CPU time waiting on I/O or stolen by the hypervisor."),
            ts("Processes running / blocked", [(f'sum(node_procs_running{{{I}}})', 'running'),
                                               (f'sum(node_procs_blocked{{{I}}})', 'blocked (D state)')], w=8,
               desc="Runnable and uninterruptibly blocked processes over the selected hosts."),
        ]),
        ("Memory", [
            ts("Memory used", [(f'1 - node_memory_MemAvailable_bytes{{{I}}} / node_memory_MemTotal_bytes{{{I}}}',
                                '{{instance}}')], unit="percentunit", mx=1, th=UTIL, th_style="dashed", w=8,
               desc="1 - MemAvailable/MemTotal. These hosts run without swap: at 100% the OOM killer acts."),
            ts("Memory composition (selected hosts)", [
                (f'sum(node_memory_MemTotal_bytes{{{I}}} - node_memory_MemFree_bytes{{{I}}} - '
                 f'node_memory_Buffers_bytes{{{I}}} - node_memory_Cached_bytes{{{I}}} - '
                 f'node_memory_SReclaimable_bytes{{{I}}})', 'used (non-reclaimable)'),
                (f'sum(node_memory_Cached_bytes{{{I}}} + node_memory_Buffers_bytes{{{I}}} + '
                 f'node_memory_SReclaimable_bytes{{{I}}})', 'page cache + buffers + reclaimable slab'),
                (f'sum(node_memory_MemFree_bytes{{{I}}})', 'free')], unit="bytes", stack=True, fill=40, w=8,
               desc="How memory is spent across the selected hosts."),
            ts("Major page faults", [(f'rate(node_vmstat_pgmajfault{{{I}}}[{RI}])', '{{instance}}')], unit="ops",
               w=8, desc="Faults that had to read from disk; a rise under memory pressure precedes thrashing."),
        ]),
        ("Disk", [
            ts("Root FS used", [(f'1 - node_filesystem_avail_bytes{{{ROOT_FS},{I}}} / '
                                 f'node_filesystem_size_bytes{{{ROOT_FS},{I}}}', '{{instance}}')],
               unit="percentunit", mx=1, th=DISK, th_style="dashed", w=8, desc="Root filesystem use per host."),
            table("Root FS free in 24h (projected)", [
                (f'predict_linear(node_filesystem_avail_bytes{{{ROOT_FS},{I}}}[6h], 86400)', '{{instance}}')],
                unit="bytes", th=steps(RED, (2 * 1024 ** 3, AMBER), (5 * 1024 ** 3, GREEN)), w=8, sort_desc=False,
                desc="Linear projection of free space 24h ahead from the last 6h trend. Negative = fills within a day."),
            ts("Disk utilisation", [(f'rate(node_disk_io_time_seconds_total{{device!~"dm-.*",{I}}}[{RI}])',
                                     '{{instance}} {{device}}')], unit="percentunit", mx=1, th=UTIL, w=8,
               desc="Share of time the device had I/O in flight."),
            ts("Disk throughput", [(f'sum by (instance) (rate(node_disk_read_bytes_total{{{I}}}[{RI}]))', '{{instance}} read'),
                                   (f'sum by (instance) (rate(node_disk_written_bytes_total{{{I}}}[{RI}]))', '{{instance}} write')],
               unit="Bps", w=12, desc="Bytes read and written per host."),
            ts("Disk latency (avg per op)", [
                (f'rate(node_disk_read_time_seconds_total{{{I}}}[{RI}]) / clamp_min(rate(node_disk_reads_completed_total{{{I}}}[{RI}]), 1e-9)',
                 '{{instance}} read'),
                (f'rate(node_disk_write_time_seconds_total{{{I}}}[{RI}]) / clamp_min(rate(node_disk_writes_completed_total{{{I}}}[{RI}]), 1e-9)',
                 '{{instance}} write')], unit="s", w=12,
               desc="Average time per completed read/write. Raft (Consul/Nomad/Vault) is sensitive to write latency."),
        ]),
        ("Network", [
            ts("eth0 receive", [(f'rate(node_network_receive_bytes_total{{device="eth0",{I}}}[{RI}])', '{{instance}}')],
               unit="Bps", w=8, desc="Bytes received on the primary interface."),
            ts("eth0 transmit", [(f'rate(node_network_transmit_bytes_total{{device="eth0",{I}}}[{RI}])', '{{instance}}')],
               unit="Bps", w=8, desc="Bytes sent on the primary interface."),
            ts("TCP retransmit ratio", [(f'rate(node_netstat_Tcp_RetransSegs{{{I}}}[{RI}]) / '
                                         f'clamp_min(rate(node_netstat_Tcp_OutSegs{{{I}}}[{RI}]), 1e-9)', '{{instance}}')],
               unit="percentunit", th=RATIO_ERR, th_style="dashed", w=8,
               desc="Retransmitted / sent segments; a lossy WireGuard or VPC path shows here first."),
            ts("TCP sockets", [(f'node_sockstat_TCP_inuse{{{I}}}', '{{instance}} in use'),
                               (f'node_sockstat_TCP_tw{{{I}}}', '{{instance}} time-wait')], w=8,
               desc="Open TCP sockets and TIME_WAIT per host."),
            ts("Conntrack table use", [(f'node_nf_conntrack_entries{{{I}}} / node_nf_conntrack_entries_limit{{{I}}}',
                                        '{{instance}}')], unit="percentunit", th=UTIL, th_style="dashed", w=8,
               desc="Connection-tracking entries / limit. The Nomad bridge and Consul tproxy NAT every flow."),
            ts("Listen drops / overflows", [(f'rate(node_netstat_TcpExt_ListenDrops{{{I}}}[{RI}])', '{{instance}} drops'),
                                            (f'rate(node_netstat_TcpExt_ListenOverflows{{{I}}}[{RI}])', '{{instance}} overflows')],
               unit="ops", w=8, desc="SYNs dropped because an accept queue was full."),
        ]),
        ("Platform services and clock", [
            empty_ok(table("Platform units not active", [
                (f'node_systemd_unit_state{{state="active",name!="podman-image-gc.service",{I}}} == 0',
                 '{{instance}} {{name}}')], th=steps(RED), w=8,
                desc="Platform units that should be running but are not. Empty is healthy.")),
            bargauge("Active platform units per host", [
                (f'sum by (instance) (node_systemd_unit_state{{state="active",{I}}})', '{{instance}}')],
                th=NEUTRAL, w=8, desc="Count of active platform units: servers run fewer than clients."),
            ts("Clock offset", [(f'node_timex_offset_seconds{{{I}}}', '{{instance}}')], unit="s", mn=None, w=8,
               desc="Kernel-reported offset from the time source."),
            table("Uptime", [(f'time() - node_boot_time_seconds{{{I}}}', '{{instance}}')], unit="s", w=8,
                  sort_desc=False, desc="Time since boot; a host that rebooted unexpectedly sorts first."),
            table("OS / kernel", [(f'node_uname_info{{{I}}}', '{{instance}} {{release}}')], w=16,
                  desc="Kernel release per host (value is always 1)."),
        ]),
    ],
    [query_var("instance", "Host", "label_values(node_uname_info, instance)")])

# ---------------------------------------------------------------- 20 nomad
NS = 'namespace=~"$namespace",exported_job=~"$job"'
POOL = 'node_pool=~"$node_pool"'
nomad = dashboard(
    "hashistack-nomad", "HashiStack / Nomad",
    "Nomad control plane (raft, eval broker, plan queue), client capacity by node pool, and per-job "
    "allocation resource use. Nomad's own job label arrives as exported_job.",
    ["nomad"],
    [
        ("Cluster", [
            stat("Autopilot unhealthy", 'count(nomad_nomad_autopilot_healthy == 0) or vector(0)', th=ZERO_GOOD,
                 graph=False, desc="Alert NomadAutopilotUnhealthy after 5m."),
            stat("Failure tolerance", 'min(nomad_nomad_autopilot_failure_tolerance)', th=TOLERANCE, graph=False,
                 desc="Servers that can fail without losing quorum. 0 = single point of failure."),
            stat("Ready clients", 'count(nomad_client_allocated_cpu{node_status="ready"})', th=ONE_GOOD,
                 desc="Clients reporting status ready."),
            stat("Ineligible clients", 'count(nomad_client_allocated_cpu{node_scheduling_eligibility!="eligible"}) '
                 'or vector(0)', th=ZERO_GOOD_AMBER, graph=False, desc="Drained or ineligible clients."),
            stat("Jobs running", 'sum(nomad_nomad_job_status_running)', desc="Jobs in status running."),
            stat("Jobs pending", 'sum(nomad_nomad_job_status_pending)', th=ZERO_GOOD_AMBER,
                 desc="Jobs waiting for placement."),
            stat("Blocked evals", 'sum(nomad_nomad_blocked_evals_total_blocked)', th=ZERO_GOOD_AMBER,
                 desc="Evaluations blocked on resources: the cluster is out of room for something."),
            stat("Failed allocs (job summaries)", 'sum(nomad_nomad_job_summary_failed)', th=ZERO_GOOD_AMBER,
                 desc="Failed allocations still counted in job summaries (cleared by GC)."),
            stat("Lost allocs", 'sum(nomad_nomad_job_summary_lost)', th=ZERO_GOOD_AMBER,
                 desc="Allocations lost with a client."),
            stat("Plan queue", 'sum(nomad_nomad_plan_queue_depth)', th=steps(GREEN, (5, AMBER), (20, RED)),
                 desc="Plans waiting for the leader to apply."),
            stat("Heartbeats active", 'sum(nomad_nomad_heartbeat_active)', desc="Client heartbeat timers on the leader."),
            stat("Agent cert runway", 'min(nomad_agent_tls_cert_expiration_seconds)', unit="s", th=AGENT_CERT,
                 desc="Soonest Nomad agent TLS certificate expiry (alert below 4h)."),
        ]),
        ("Scheduler", [
            ts("Eval broker", [('sum(nomad_nomad_broker_total_ready)', 'ready'),
                               ('sum(nomad_nomad_broker_total_unacked)', 'unacked'),
                               ('sum(nomad_nomad_broker_total_pending)', 'pending'),
                               ('sum(nomad_nomad_broker_total_waiting)', 'waiting'),
                               ('sum(nomad_nomad_blocked_evals_total_blocked)', 'blocked')], w=8,
               desc="Evaluations by broker state. Growing ready/unacked means workers cannot keep up."),
            ts("Plan apply / evaluate p99", [('max(nomad_nomad_plan_apply{quantile="0.99"})', 'apply'),
                                             ('max(nomad_nomad_plan_evaluate{quantile="0.99"})', 'evaluate'),
                                             ('max(nomad_nomad_plan_submit{quantile="0.99"})', 'submit')],
               unit="ms", w=8, desc="Leader plan pipeline latency (go-metrics summary, last interval)."),
            ts("Server RPC rate", [(f'sum(rate(nomad_nomad_rpc_request[{RI}]))', 'requests'),
                                   (f'sum(rate(nomad_nomad_rpc_query[{RI}]))', 'queries'),
                                   (f'sum(rate(nomad_nomad_rpc_accept_conn[{RI}]))', 'new connections')],
               unit="ops", w=8, desc="RPCs handled by the servers."),
        ]),
        ("Raft and server runtime", [
            ts("Raft commit time p99", [('max by (instance) (nomad_raft_commitTime{quantile="0.99"})', '{{instance}}')],
               unit="ms", w=6, desc="Time to commit a log entry to quorum. Disk or network latency shows here."),
            ts("Raft applies", [(f'sum by (instance) (rate(nomad_raft_apply[{RI}]))', '{{instance}}')], unit="ops",
               w=6, desc="Raft transactions per second."),
            ts("Raft thread saturation", [('max by (instance) (nomad_raft_thread_main_saturation{quantile="0.99"})', '{{instance}} main'),
                                          ('max by (instance) (nomad_raft_thread_fsm_saturation{quantile="0.99"})', '{{instance}} fsm')],
               unit="percentunit", mx=1, th=UTIL, w=6, desc="How busy the raft main and FSM goroutines are."),
            ts("Server goroutines / heap", [('nomad_runtime_num_goroutines{service="nomad"}', '{{instance}} goroutines'),
                                            ('nomad_runtime_alloc_bytes{service="nomad"} / 1048576', '{{instance}} heap MiB')],
               w=6, desc="Steadily growing goroutines or heap on a server means a leak."),
        ]),
        ("Client capacity", [
            bargauge("CPU reserved by client", [(f'nomad_client_allocated_cpu{{{POOL}}} / (nomad_client_allocated_cpu{{{POOL}}} + '
                                                 f'nomad_client_unallocated_cpu{{{POOL}}})', '{{instance}} ({{node_pool}})')],
                     unit="percentunit", mx=1, desc="Reserved / schedulable CPU per client now."),
            bargauge("Memory reserved by client", [(f'nomad_client_allocated_memory{{{POOL}}} / (nomad_client_allocated_memory{{{POOL}}} + '
                                                    f'nomad_client_unallocated_memory{{{POOL}}})', '{{instance}} ({{node_pool}})')],
                     unit="percentunit", mx=1, desc="Reserved / schedulable memory per client now."),
            bargauge("Unreserved memory by client", [(f'nomad_client_unallocated_memory{{{POOL}}} * 1048576',
                                                      '{{instance}} ({{node_pool}})')],
                     unit="bytes", th=steps(RED, (512 * 1048576, AMBER), (1024 ** 3, GREEN)),
                     desc="Memory still free for new allocations. A job asking for more than this cannot land."),
            ts("Allocations running per client", [(f'nomad_client_allocations_running{{{POOL}}}', '{{instance}}')],
               w=12, desc="Running allocations per client."),
            # One series per core, and node_status is a label: averaging by
            # instance keeps one line per client when a client's status changes.
            ts("Client host CPU vs reserved", [
                (f'avg by (instance) (nomad_client_host_cpu_total_percent{{{POOL}}}) / 100', '{{instance}} used'),
            ], unit="percentunit", mx=1, w=12,
               desc="Actual host CPU per client as Nomad sees it, averaged over its cores; compare with the "
               "reserved bar above to spot over- or under-reservation."),
        ]),
        ("Workloads (selected namespace / job)", [
            ts("CPU by job", [(f'sum by (namespace, exported_job) (nomad_client_allocs_cpu_total_percent{{{NS}}}) / 100',
                               '{{namespace}}/{{exported_job}}')], unit="percentunit", w=12,
               desc="CPU used by each job's tasks, in cores (1.0 = one core)."),
            ts("Memory by job", [(f'sum by (namespace, exported_job) (nomad_client_allocs_memory_usage{{{NS}}})',
                                  '{{namespace}}/{{exported_job}}')], unit="bytes", w=12,
               desc="Memory used (cgroup usage) by each job's tasks."),
            bargauge("Tasks nearest their memory reservation", [
                (f'topk(15, nomad_client_allocs_memory_usage{{{NS}}} / nomad_client_allocs_memory_allocated{{{NS}}})',
                 '{{namespace}}/{{exported_job}}/{{task}}')], unit="percentunit", w=12, h=9,
                th=steps(GREEN, (0.8, AMBER), (1, RED)),
                desc="Usage / reserved memory per task. Above 100% the task is living on memory_max "
                "oversubscription and is first to be OOM-killed under pressure."),
            bargauge("Top CPU tasks", [(f'topk(15, nomad_client_allocs_cpu_total_percent{{{NS}}} / 100)',
                                        '{{namespace}}/{{exported_job}}/{{task}}')], unit="percentunit", w=12, h=9,
                     th=steps(GREEN, (0.75, AMBER), (1, RED)), desc="Busiest tasks, in cores."),
            ts("CPU throttling", [(f'topk(10, sum by (namespace, exported_job, task) '
                                   f'(rate(nomad_client_allocs_cpu_throttled_periods{{{NS}}}[{RI}])))',
                                   '{{namespace}}/{{exported_job}}/{{task}}')], unit="ops", w=12,
               desc="CFS periods in which a task hit its CPU limit. Persistent throttling = slow requests."),
            ts("Memory by task (top 10)", [(f'topk(10, nomad_client_allocs_memory_usage{{{NS}}})',
                                            '{{namespace}}/{{exported_job}}/{{task}}')], unit="bytes", w=12,
               desc="Largest tasks by memory."),
        ]),
    ],
    [query_var("node_pool", "Node pool", "label_values(nomad_client_allocated_cpu, node_pool)"),
     query_var("namespace", "Namespace", "label_values(nomad_client_allocs_memory_usage, namespace)"),
     query_var("job", "Job", 'label_values(nomad_client_allocs_memory_usage{namespace=~"$namespace"}, exported_job)')])

# ---------------------------------------------------------------- 30 consul
CI = 'instance=~"$instance"'
consul = dashboard(
    "hashistack-consul", "HashiStack / Consul",
    "Consul servers and agents: autopilot/raft, RPC and rate limiting, catalog size, KV and ACL latency, "
    "agent and Connect CA certificate runway, xDS streams to the Envoy sidecars.",
    ["consul"],
    [
        ("Cluster", [
            stat("Leaders", 'sum(consul_server_isLeader)', th=EXACTLY_ONE, graph=False,
                 desc="Servers that believe they are leader. Must be exactly 1."),
            stat("Autopilot unhealthy", 'count(consul_autopilot_healthy == 0) or vector(0)', th=ZERO_GOOD,
                 graph=False, desc="Alert ConsulAutopilotUnhealthy after 5m."),
            stat("Failure tolerance", 'min(consul_autopilot_failure_tolerance)', th=TOLERANCE, graph=False,
                 desc="Servers that can fail without losing quorum."),
            stat("Servers / clients", 'max(consul_members_servers) + max(consul_members_clients)', graph=False,
                 desc="LAN members (servers + clients) known to the cluster."),
            stat("Services", 'max(consul_state_services)', desc="Registered services in the catalog."),
            stat("Service instances", 'max(consul_state_service_instances)', desc="Service instances in the catalog."),
            stat("Connect instances", 'max(consul_state_connect_instances)', desc="Mesh (Connect) instances."),
            stat("KV entries", 'max(consul_state_kv_entries)', desc="Keys in the KV store."),
            stat("Sidecars off xDS", 'count(envoy_control_plane_connected_state == 0) or vector(0)', th=ZERO_GOOD,
                 graph=False, desc="Envoy sidecars not connected to their Consul agent's xDS server: they keep "
                 "serving their last config but miss intention, upstream and certificate updates. (Read from Envoy: "
                 "consul_xds_server_streams reports 0 on these agents.)"),
            stat("Agent cert runway", 'min(consul_agent_tls_cert_expiry)', unit="s", th=AGENT_CERT,
                 desc="Soonest Consul agent TLS certificate expiry (alert below 4h)."),
            stat("Connect root CA", 'min(consul_mesh_active_root_ca_expiry)', unit="s", th=CA_CERT,
                 desc="Runway of the active Connect root CA (alert below 30d)."),
            stat("Connect signing CA", 'min(consul_mesh_active_signing_ca_expiry)', unit="s", th=CA_CERT,
                 desc="Runway of the Vault-backed intermediate that signs mesh leaves."),
        ]),
        ("Raft and RPC", [
            ts("Raft commit time p99", [('max by (instance) (consul_raft_commitTime{quantile="0.99"})', '{{instance}}')],
               unit="ms", w=6, desc="Time to commit a log entry to quorum."),
            ts("Raft applies", [(f'sum by (instance) (rate(consul_raft_apply[{RI}]))', '{{instance}}')], unit="ops",
               w=6, desc="Raft transactions per second."),
            ts("Server RPC", [(f'sum(rate(consul_rpc_request[{RI}]))', 'requests'),
                              (f'sum(rate(consul_rpc_query[{RI}]))', 'queries'),
                              (f'sum(rate(consul_rpc_request_error[{RI}]))', 'errors'),
                              (f'sum(rate(consul_rpc_rate_limit_exceeded[{RI}]))', 'rate limited')], unit="ops",
               w=6, desc="RPCs reaching the servers, errors and requests refused by the rate limiter."),
            ts("Blocking queries", [('sum by (instance) (consul_rpc_queries_blocking)', '{{instance}}')], w=6,
               desc="Long-poll queries held open on each server (Nomad, vault-agent and consul-template watches)."),
            ts("Agent → server RPC failures", [(f'sum by (instance) (rate(consul_client_rpc_failed{{{CI}}}[{RI}]))',
                                                 '{{instance}} failed'),
                                                (f'sum by (instance) (rate(consul_client_rpc_exceeded{{{CI}}}[{RI}]))',
                                                 '{{instance}} rate-limited')],
               unit="ops", w=12, desc="Client agents failing or being throttled talking to the servers."),
            ts("Agent HTTP API p99 (top 10 paths)", [
                (f'topk(10, max by (method, path) (consul_api_http{{quantile="0.99",{CI}}}))', '{{method}} {{path}}')],
               unit="ms", w=12, desc="Slowest local-agent HTTP endpoints. Blocking catalog/health calls are "
               "long by design."),
        ]),
        ("KV, catalog and ACL", [
            ts("KV / txn apply p99", [('max(consul_kvs_apply{quantile="0.99"})', 'kvs apply'),
                                      ('max(consul_txn_apply{quantile="0.99"})', 'txn apply')], unit="ms", w=6,
               desc="Write latency of KV and transaction endpoints on the leader. Gaps = no writes in the interval."),
            ts("Catalog writes", [(f'sum(rate(consul_catalog_register_count[{RI}]))', 'register'),
                                  (f'sum(rate(consul_catalog_deregister_count[{RI}]))', 'deregister')],
               unit="ops", w=6, desc="Service registration churn. Spikes follow deploys or flapping allocations."),
            ts("ACL resolve p99", [('max by (instance) (consul_acl_ResolveToken{quantile="0.99"})', '{{instance}}')],
               unit="ms", w=6, desc="Time to resolve an ACL token, per agent."),
            ts("ACL token cache miss ratio", [(f'sum(rate(consul_acl_token_cache_miss[{RI}])) / clamp_min(sum(rate('
                                               f'consul_acl_token_cache_hit[{RI}])) + sum(rate(consul_acl_token_cache_miss[{RI}])), 1e-9)',
                                               'miss ratio')], unit="percentunit", mx=1, w=6,
               desc="Share of token lookups that missed the agent cache and went to the servers."),
        ]),
        ("Certificates and mesh control", [
            table("Agent TLS cert runway", [(f'consul_agent_tls_cert_expiry{{{CI}}}', '{{instance}}')], unit="s",
                  th=AGENT_CERT, sort_desc=False, w=8, desc="Seconds until each agent's certificate expires."),
            ts("xDS-connected sidecars per node", [('sum by (instance) (envoy_control_plane_connected_state)', '{{instance}}')],
               w=8, desc="Envoys connected to the local agent's xDS server, per node. A node dropping to zero "
               "means its agent stopped serving config."),
            ts("gRPC streams / connections", [('sum by (instance) (consul_grpc_server_streams)', '{{instance}} streams'),
                                              ('sum by (instance) (consul_grpc_server_connections)', '{{instance}} connections')],
               w=8, desc="gRPC (xDS, agentless and peering) streams and connections held by the servers."),
        ]),
        ("Agent runtime", [
            ts("Goroutines", [(f'consul_runtime_num_goroutines{{{CI}}}', '{{instance}}')], w=8,
               desc="Goroutines per agent; servers run more."),
            ts("Heap allocated", [(f'consul_runtime_alloc_bytes{{{CI}}}', '{{instance}}')], unit="bytes", w=8,
               desc="Go heap in use per agent."),
            ts("Memberlist gossip p99", [(f'consul_memberlist_gossip{{quantile="0.99",{CI}}}', '{{instance}}')],
               unit="ms", w=8, desc="Gossip round time; high values precede flapping members."),
        ]),
    ],
    [query_var("instance", "Agent", "label_values(consul_runtime_num_goroutines, instance)")])

# ---------------------------------------------------------------- 40 vault
vault = dashboard(
    "hashistack-vault", "HashiStack / Vault",
    "Vault seal/HA state, request load and latency, tokens and leases, PKI issuance per mount, "
    "integrated-storage (raft) health and runtime.",
    ["vault"],
    [
        ("Status", [
            stat("Sealed", 'count(vault_core_unsealed == 0) or vector(0)', th=ZERO_GOOD, graph=False,
                 desc="Sealed Vault servers (alert VaultSealed after 1m)."),
            stat("Active nodes", 'sum(vault_core_active)', th=EXACTLY_ONE, graph=False,
                 desc="Servers in active (not standby) mode. Must be exactly 1."),
            stat("Autopilot unhealthy", 'count(vault_autopilot_healthy == 0) or vector(0)', th=ZERO_GOOD, graph=False,
                 desc="Alert VaultAutopilotUnhealthy after 5m."),
            stat("Failure tolerance", 'min(vault_autopilot_failure_tolerance)', th=TOLERANCE, graph=False,
                 desc="Raft peers that can fail without losing quorum."),
            stat("Requests /s", f'sum(rate(vault_core_handle_request_count[{RI}]))', unit="reqps", decimals=1,
                 desc="Authenticated requests handled."),
            stat("Error ratio", f'(sum(rate(vault_core_response_status_code{{type=~"4xx|5xx"}}[{RI}])) or vector(0)) / '
                 f'clamp_min(sum(rate(vault_core_response_status_code[{RI}])), 1e-9)', unit="percentunit",
                 th=RATIO_ERR, decimals=2, desc="4xx + 5xx share of responses. A rise usually means an expired "
                 "token or a denied policy path."),
            stat("In-flight requests", 'sum(vault_core_in_flight_requests)', th=steps(GREEN, (20, AMBER), (100, RED)),
                 desc="Requests being processed now."),
            stat("Leases", 'sum(vault_expire_num_leases)', desc="Leases tracked by the expiration manager."),
            stat("Irrevocable leases", 'sum(vault_expire_num_irrevocable_leases)', th=ZERO_GOOD_AMBER,
                 desc="Leases Vault failed to revoke; their secrets stay live until cleaned up."),
            stat("Identity entities", 'sum(last_over_time(vault_identity_num_entities[15m]))', desc="Identity entities."),
            stat("Raft FSM pending", 'sum(vault_raft_storage_stats_fsm_pending)', th=steps(GREEN, (10, AMBER), (100, RED)),
                 desc="Committed raft entries not yet applied to the FSM."),
            stat("Expiration queue", 'sum(last_over_time(vault_expire_job_manager_queue_length[15m]))', th=steps(GREEN, (100, AMBER), (1000, RED)),
                 desc="Lease expirations waiting for a worker."),
        ]),
        ("Requests", [
            ts("Request rate", [(f'sum(rate(vault_core_handle_request_count[{RI}]))', 'requests'),
                                (f'sum(rate(vault_core_handle_login_request_count[{RI}]))', 'logins')], unit="reqps",
               w=8, desc="Requests and logins per second."),
            ts("Request latency", [('max(vault_core_handle_request{quantile="0.99"})', 'p99'),
                                   ('max(vault_core_handle_request{quantile="0.5"})', 'p50'),
                                   (f'sum(rate(vault_core_handle_request_sum[{RI}])) / clamp_min(sum(rate(vault_core_handle_request_count[{RI}])), 1e-9)',
                                    'mean')], unit="ms", w=8, desc="handle_request latency."),
            ts("Responses by status code", [(f'sum by (code) (rate(vault_core_response_status_code[{RI}]))', '{{code}}')],
               unit="reqps", stack=True, fill=30, w=8, desc="Responses by HTTP status. A steady 403 stream is a "
               "client retrying with a token or policy that no longer grants the path; 429 is rate limiting."),
        ]),
        ("PKI issuance", [
            ts("Certificates issued per mount", [
                (f'sum(rate(vault_consul_pki_issue_count[{RI}]))', 'consul_pki'),
                (f'sum(rate(vault_nomad_pki_issue_count[{RI}]))', 'nomad_pki'),
                (f'sum(rate(vault_edge_pki_issue_count[{RI}]))', 'edge_pki'),
                (f'sum(rate(vault_prometheus_pki_issue_count[{RI}]))', 'prometheus_pki'),
                (f'sum(rate(vault_connect_inter_sign_count[{RI}]))', 'connect_inter (mesh leaves)')],
               unit="ops", w=12, desc="Issue/sign calls per PKI mount: vault-agent renewals, Connect leaf "
               "signing, reader certs."),
            ts("PKI issue latency p99", [('max(vault_consul_pki_issue{quantile="0.99"})', 'consul_pki'),
                                         ('max(vault_nomad_pki_issue{quantile="0.99"})', 'nomad_pki'),
                                         ('max(vault_edge_pki_issue{quantile="0.99"})', 'edge_pki'),
                                         ('max(vault_connect_inter_sign{quantile="0.99"})', 'connect_inter')],
               unit="ms", w=12, desc="Key generation + signing time per mount."),
        ]),
        ("Tokens and leases", [
            ts("Token operations", [(f'sum(rate(vault_token_creation[{RI}]))', 'created'),
                                    (f'sum(rate(vault_token_lookup_count[{RI}]))', 'lookups'),
                                    (f'sum(rate(vault_token_revoke_tree_count[{RI}]))', 'revoke-tree')], unit="ops",
               w=8, desc="Token churn. A creation spike with no matching revokes grows the lease table."),
            ts("Leases", [('sum(vault_expire_num_leases)', 'leases'),
                          ('sum(vault_expire_num_irrevocable_leases)', 'irrevocable')], w=8,
               desc="Lease count over time."),
            ts("Lease revocations", [(f'sum(rate(vault_expire_revoke_count[{RI}]))', 'revoke'),
                                     (f'sum(rate(vault_expire_renew_token_count[{RI}]))', 'renew token')],
               unit="ops", w=8, desc="Lease revocations and token renewals."),
        ]),
        ("Storage (raft) and runtime", [
            ts("Raft commit / storage latency p99", [
                ('max(vault_raft_commitTime{quantile="0.99"})', 'raft commit'),
                ('max(vault_raft_storage_put{quantile="0.99"})', 'storage put'),
                ('max(vault_raft_storage_get{quantile="0.99"})', 'storage get'),
                ('max(vault_barrier_put{quantile="0.99"})', 'barrier put')], unit="ms", w=12,
               desc="Integrated storage latency. NaN gaps = no operations in the interval."),
            ts("Raft index", [(f'rate(vault_raft_storage_stats_applied_index[{RI}])', 'applied index /s'),
                              (f'rate(vault_raft_storage_stats_commit_index[{RI}])', 'commit index /s')],
               unit="ops", w=12, desc="Write rate into the raft log."),
            ts("Goroutines", [('vault_runtime_num_goroutines', '{{instance}}')], w=8, desc="Vault goroutines."),
            ts("Heap allocated", [('vault_runtime_alloc_bytes', '{{instance}}')], unit="bytes", w=8,
               desc="Go heap in use."),
            ts("GC pause", [(f'rate(vault_runtime_total_gc_pause_ns[{RI}]) / 1e9', '{{instance}}')],
               unit="percentunit", w=8, desc="Fraction of wall time spent in GC pauses."),
        ]),
    ])

# ---------------------------------------------------------------- 50 service mesh
S = 'nomad_namespace=~"$namespace",service=~"$service"'
mesh = dashboard(
    "hashistack-service-mesh", "HashiStack / Service Mesh (Envoy)",
    "Consul Connect sidecars on every platform root: inbound HTTP golden signals at the public listener, "
    "TCP traffic, upstream health, mTLS handshake errors and sidecar resources. `service` is the parent "
    "service, not the -sidecar-proxy registration.",
    ["envoy", "consul-connect"],
    [
        ("Golden signals (selected sidecars)", [
            stat("Sidecars live", f'sum(envoy_server_live{{{S}}})', th=ONE_GOOD, desc="Envoys reporting LIVE."),
            stat("Inbound HTTP", f'sum(rate(envoy_http_downstream_rq_total{{{PUB},{S}}}[{RI}]))', unit="reqps",
                 decimals=2, desc="Requests reaching the public listener of the selected HTTP services."),
            stat("5xx ratio", f'(sum(rate(envoy_http_downstream_rq_xx{{{PUB},envoy_response_code_class="5",{S}}}[{RI}])) '
                 f'or vector(0)) / clamp_min(sum(rate(envoy_http_downstream_rq_total{{{PUB},{S}}}[{RI}])), 1e-9)',
                 unit="percentunit", th=RATIO_ERR, decimals=2, desc="Share of inbound requests answered 5xx."),
            stat("Inbound p95", f'histogram_quantile(0.95, sum by (le) (rate(envoy_http_downstream_rq_time_bucket'
                 f'{{{PUB},{S}}}[{LAT}])))', unit="ms", th=steps(GREEN, (250, AMBER), (1000, RED)),
                 desc="95th percentile request time at the public listener (sidecar to app and back)."),
            stat("Active connections", f'sum(envoy_listener_downstream_cx_active{{{S}}})',
                 desc="Open downstream connections on every listener of the selected sidecars."),
            stat("App connect failures", f'sum(rate(envoy_cluster_upstream_cx_connect_fail{{envoy_cluster_name="local_app",{S}}}[{RI}]))',
                 unit="ops", th=ZERO_GOOD_AMBER, decimals=2,
                 desc="Sidecar could not connect to its own app (local_app). The app is down or not listening."),
            stat("Upstream connect failures", f'sum(rate(envoy_cluster_upstream_cx_connect_fail{{{UPSTREAMS},{S}}}[{RI}]))',
                 unit="ops", th=ZERO_GOOD_AMBER, decimals=2,
                 desc="Failed connections to mesh upstreams (intention denials surface at the source as these)."),
            stat("Unhealthy upstream members", f'sum(envoy_cluster_membership_total{{{UPSTREAMS},{S}}}) - '
                 f'sum(envoy_cluster_membership_healthy{{{UPSTREAMS},{S}}})', th=ZERO_GOOD_AMBER,
                 desc="Upstream endpoints Envoy knows about but considers unhealthy."),
            stat("mTLS errors /s", f'sum(rate(envoy_listener_ssl_connection_error{{{S}}}[{RI}])) + '
                 f'sum(rate(envoy_cluster_ssl_connection_error{{{S}}}[{RI}]))', unit="ops", th=ZERO_GOOD_AMBER,
                 decimals=2, desc="TLS failures on inbound listeners and outbound clusters."),
            stat("Circuit-breaker overflows", f'sum(rate(envoy_cluster_upstream_cx_overflow{{{S}}}[{RI}])) + '
                 f'sum(rate(envoy_cluster_upstream_rq_pending_overflow{{{S}}}[{RI}]))', unit="ops",
                 th=ZERO_GOOD_AMBER, decimals=2, desc="Connections/requests shed by Envoy circuit breakers."),
            stat("Leaf cert runway", f'min({LEAF_EXPIRY}{{{S}}} > 0) - time()', unit="s", th=LEAF_CERT,
                 desc="Seconds until the soonest inbound mTLS leaf of the selected sidecars expires."),
            stat("Sidecar memory", f'sum(envoy_server_memory_allocated{{{S}}})', unit="bytes",
                 desc="Heap allocated across the selected Envoys."),
        ]),
        ("Inbound HTTP (public listener)", [
            ts("Requests by service", [(f'sum by (service) (rate(envoy_http_downstream_rq_total{{{PUB},{S}}}[{RI}]))',
                                        '{{service}}')], unit="reqps", w=12, desc="Inbound HTTP request rate."),
            ts("Errors by service", [(f'sum by (service, envoy_response_code_class) (rate(envoy_http_downstream_rq_xx'
                                      f'{{{PUB},envoy_response_code_class=~"4|5",{S}}}[{RI}]))',
                                      '{{service}} {{envoy_response_code_class}}xx')], unit="reqps", w=12,
               desc="4xx and 5xx responses per service."),
            ts("Inbound latency", [
                (f'histogram_quantile(0.50, sum by (le) (rate(envoy_http_downstream_rq_time_bucket{{{PUB},{S}}}[{LAT}])))', 'p50'),
                (f'histogram_quantile(0.95, sum by (le) (rate(envoy_http_downstream_rq_time_bucket{{{PUB},{S}}}[{LAT}])))', 'p95'),
                (f'histogram_quantile(0.99, sum by (le) (rate(envoy_http_downstream_rq_time_bucket{{{PUB},{S}}}[{LAT}])))', 'p99')],
               unit="ms", w=12, desc="Request time quantiles across the selected services."),
            ts("p95 latency by service", [
                (f'histogram_quantile(0.95, sum by (service, le) (rate(envoy_http_downstream_rq_time_bucket{{{PUB},{S}}}[{LAT}])))',
                 '{{service}}')], unit="ms", w=12, desc="Slowest services float to the top of the legend."),
        ]),
        ("TCP and connections", [
            ts("TCP connections opened", [(f'sum by (service) (rate(envoy_tcp_downstream_cx_total{{{S}}}[{RI}]))',
                                           '{{service}}')], unit="ops", w=8,
               desc="New TCP-proxied connections (postgres, ssh, OCI doors and other non-HTTP services)."),
            ts("TCP throughput", [(f'sum by (service) (rate(envoy_tcp_downstream_cx_rx_bytes_total{{{S}}}[{RI}]) + '
                                   f'rate(envoy_tcp_downstream_cx_tx_bytes_total{{{S}}}[{RI}]))', '{{service}}')],
               unit="Bps", w=8, desc="Bytes in + out through TCP proxies."),
            ts("Active downstream connections", [(f'sum by (service) (envoy_listener_downstream_cx_active{{{S}}})',
                                                  '{{service}}')], w=8, desc="Open connections per sidecar."),
        ]),
        ("Upstreams (outbound)", [
            ts("Active upstream connections", [(f'sum by (service, envoy_cluster_name) (envoy_cluster_upstream_cx_active'
                                                f'{{{UPSTREAMS},{S}}}) > 0', '{{service}} → {{envoy_cluster_name}}')],
               w=12, desc="Connections each service holds to its mesh upstreams."),
            empty_ok(ts("Upstream connect failures / timeouts", [
                (f'sum by (service, envoy_cluster_name) (rate(envoy_cluster_upstream_cx_connect_fail{{{UPSTREAMS},{S}}}[{RI}]) + '
                 f'rate(envoy_cluster_upstream_cx_connect_timeout{{{UPSTREAMS},{S}}}[{RI}])) > 0',
                 '{{service}} → {{envoy_cluster_name}}')], unit="ops", w=12,
               desc="Failed or timed-out upstream connects. Empty is healthy.")),
            empty_ok(ts("Upstream HTTP 5xx", [(f'sum by (service, envoy_cluster_name) (rate(envoy_cluster_upstream_rq_xx'
                                      f'{{envoy_response_code_class="5",{S}}}[{RI}])) > 0',
                                      '{{service}} → {{envoy_cluster_name}}')], unit="reqps", w=12,
               desc="5xx returned by upstreams, including the sidecar's own app (local_app).")),
            ts("Retries and resets", [(f'sum by (service) (rate(envoy_cluster_upstream_rq_retry{{{S}}}[{RI}]))', '{{service}} retries'),
                                      (f'sum by (service) (rate(envoy_cluster_upstream_rq_rx_reset{{{S}}}[{RI}]))', '{{service}} upstream resets')],
               unit="ops", w=12, desc="Retries and upstream-reset requests."),
            table("Upstream endpoint health", [
                (f'sum by (service, envoy_cluster_name) (envoy_cluster_membership_healthy{{{UPSTREAMS},{S}}}) / '
                 f'sum by (service, envoy_cluster_name) (envoy_cluster_membership_total{{{UPSTREAMS},{S}}})',
                 '{{service}} → {{envoy_cluster_name}}')], unit="percentunit", th=steps(RED, (0.5, AMBER), (1, GREEN)),
                sort_desc=False, w=24, h=10, desc="Healthy / known endpoints per upstream cluster. NaN = no endpoints "
                "(the upstream is not registered)."),
        ]),
        ("mTLS and sidecar resources", [
            ts("Inbound mTLS handshakes", [(f'sum by (service) (rate(envoy_listener_ssl_handshake{{{S}}}[{RI}]))',
                                            '{{service}}')], unit="ops", w=8, desc="Successful inbound TLS handshakes."),
            empty_ok(ts("TLS failures", [(f'sum by (service) (rate(envoy_listener_ssl_connection_error{{{S}}}[{RI}]) + '
                                 f'rate(envoy_listener_ssl_fail_verify_error{{{S}}}[{RI}]) + '
                                 f'rate(envoy_listener_ssl_fail_verify_san{{{S}}}[{RI}]) + '
                                 f'rate(envoy_listener_ssl_fail_verify_no_cert{{{S}}}[{RI}])) > 0', '{{service}} inbound'),
                                (f'sum by (service) (rate(envoy_cluster_ssl_connection_error{{{S}}}[{RI}])) > 0',
                                 '{{service}} outbound')], unit="ops", w=8,
               desc="Handshake, verification, SAN and missing-certificate failures. Empty is healthy.")),
            bargauge("Leaf cert runway by service", [(f'min by (service) ({LEAF_EXPIRY}{{{S}}} > 0) - time()',
                                                      '{{service}}')], unit="s", th=LEAF_CERT, w=8,
                     desc="Seconds until each sidecar's inbound mTLS leaf expires."),
            ts("Sidecar memory", [(f'sum by (service) (envoy_server_memory_allocated{{{S}}})', '{{service}}')],
               unit="bytes", w=12, desc="Envoy heap per sidecar."),
            table("Sidecar uptime", [(f'min by (service) (envoy_server_uptime{{{S}}})', '{{service}}')], unit="s",
                  sort_desc=False, w=12, desc="Seconds since each Envoy started. A recent restart sorts first "
                  "(an in-place restart can come back with an anonymous token)."),
        ]),
    ],
    [query_var("namespace", "Namespace", "label_values(envoy_server_live, nomad_namespace)"),
     query_var("service", "Service", 'label_values(envoy_server_live{nomad_namespace=~"$namespace"}, service)')])

# ---------------------------------------------------------------- 60 RDW
# The devpod matcher stays even under "All" (allValue .*): without it All means every job in the cluster.
DEVPOD = '.+-devpod-.+'
WS = f'namespace=~"$developer",exported_job=~"$workspace",exported_job=~"{DEVPOD}"'
WSC = f'envoy_cluster_name=~"$workspace",envoy_cluster_name=~"{DEVPOD}"'
WS_MAIN = f'{WS},exported_job!~".+-db"'
rdw = dashboard(
    "hashistack-rdw-workspaces", "HashiStack / Remote Dev Workspaces",
    "Developer workspaces (Nomad jobs <developer>-devpod-<seed> in each developer's namespace) and the "
    "planes they depend on: resource use against reservations, SSH (sm-jump bastion) and web (Traefik) "
    "access, onboarding portal/TUI, Ory identity, GitLab and Nexus.",
    ["rdw", "devpod"],
    [
        ("Fleet (selected developers)", [
            stat("Developers", f'count(count by (namespace) (nomad_client_allocs_memory_usage{{{WS}}}))',
                 desc="Developer namespaces with at least one running workspace."),
            stat("Workspaces running", f'count(count by (namespace, exported_job) (nomad_client_allocs_memory_usage{{{WS_MAIN}}}))',
                 desc="Running workspace jobs (companion databases excluded)."),
            stat("Companion DBs", f'count(count by (namespace, exported_job) (nomad_client_allocs_memory_usage'
                 f'{{{WS},exported_job=~".+-db"}})) or vector(0)', desc="Running *-db companion jobs."),
            stat("Workspace CPU", f'sum(nomad_client_allocs_cpu_total_percent{{{WS}}}) / 100', unit="short", decimals=2,
                 desc="Cores in use by every task of the selected workspaces."),
            stat("Workspace memory", f'sum(nomad_client_allocs_memory_usage{{{WS}}})', unit="bytes",
                 desc="Memory in use by the selected workspaces and companions."),
            stat("Memory vs reservation", f'sum(nomad_client_allocs_memory_usage{{{WS}}}) / '
                 f'sum(nomad_client_allocs_memory_allocated{{{WS}}})', unit="percentunit", th=UTIL, decimals=0,
                 desc="Used / reserved memory over the selected workspaces."),
            stat("SSH sessions", f'sum(envoy_cluster_upstream_cx_active{{service="sm-jump-bastion",{WSC}}}) or vector(0)',
                 desc="Open bastion → workspace connections (sm-jump)."),
            stat("Web connections", f'sum(envoy_cluster_upstream_cx_active{{service="sm-ingress-traefik",{WSC}}}) or vector(0)',
                 desc="Open Traefik → workspace connections (browser IDE)."),
            stat("Workspace connect failures", f'sum(rate(envoy_cluster_upstream_cx_connect_fail{{service=~"sm-jump-bastion|'
                 f'sm-ingress-traefik",{WSC}}}[{RI}])) or vector(0)', unit="ops", th=ZERO_GOOD_AMBER, decimals=2,
                 desc="Bastion or Traefik failing to reach a workspace (stopped workspace or denied intention)."),
            stat("Onboarding 5xx ratio", f'(sum(rate(envoy_http_downstream_rq_xx{{{PUB},envoy_response_code_class="5",'
                 f'nomad_namespace="remote-dev-ws-onboarding"}}[{RI}])) or vector(0)) / clamp_min(sum(rate('
                 f'envoy_http_downstream_rq_total{{{PUB},nomad_namespace="remote-dev-ws-onboarding"}}[{RI}])), 1e-9)',
                 unit="percentunit", th=RATIO_ERR, decimals=2, desc="Portal, TUI and Oathkeeper inbound 5xx share."),
            stat("Identity 5xx ratio", f'(sum(rate(envoy_http_downstream_rq_xx{{{PUB},envoy_response_code_class="5",'
                 f'nomad_namespace="ory-identity"}}[{RI}])) or vector(0)) / clamp_min(sum(rate('
                 f'envoy_http_downstream_rq_total{{{PUB},nomad_namespace="ory-identity"}}[{RI}])), 1e-9)',
                 unit="percentunit", th=RATIO_ERR, decimals=2, desc="Kratos, Hydra and login-consent inbound 5xx share."),
            stat("Default-pool memory free", 'sum(nomad_client_unallocated_memory{node_pool="default"}) * 1048576',
                 unit="bytes", th=steps(RED, (2 * 1024 ** 3, AMBER), (4 * 1024 ** 3, GREEN)),
                 desc="Unreserved memory in the default node pool, where workspaces are placed. A new workspace "
                 "needs its reservation to fit on one node."),
        ]),
        ("Workspace resources", [
            ts("CPU by workspace", [(f'sum by (exported_job) (nomad_client_allocs_cpu_total_percent{{{WS}}}) / 100',
                                     '{{exported_job}}')], unit="short", decimals=2, w=12,
               desc="Cores used per workspace job (all tasks: IDE, git-broker, llm-broker, Envoy)."),
            ts("Memory by workspace", [(f'sum by (exported_job) (nomad_client_allocs_memory_usage{{{WS}}})',
                                        '{{exported_job}}')], unit="bytes", w=12, desc="Memory used per workspace job."),
            bargauge("Memory vs reservation by task", [
                (f'nomad_client_allocs_memory_usage{{{WS}}} / nomad_client_allocs_memory_allocated{{{WS}}}',
                 '{{exported_job}}/{{task}}')], unit="percentunit", th=steps(GREEN, (0.8, AMBER), (1, RED)), w=12, h=9,
                desc="Above 100% a task runs on memory_max oversubscription and is first to be OOM-killed."),
            bargauge("Memory by task role", [
                (f'sum by (role) (label_replace(label_replace(label_replace(label_replace('
                 f'nomad_client_allocs_memory_usage{{{WS}}}, "role", "other", "task", ".*"), '
                 f'"role", "workspace (IDE)", "task", ".*-devpod-.*"), "role", "envoy sidecar", "task", "connect-proxy-.*"), '
                 f'"role", "$1", "task", "(git-broker|llm-broker|db)"))', '{{role}}')],
                unit="bytes", th=NEUTRAL, w=12, h=9,
                desc="Where workspace memory goes: the IDE container vs. the git/LLM brokers, companion databases "
                "and Envoy sidecars."),
            ts("CPU throttling by workspace", [(f'sum by (exported_job) (rate(nomad_client_allocs_cpu_throttled_periods'
                                                f'{{{WS}}}[{RI}]))', '{{exported_job}}')], unit="ops", w=12,
               desc="CFS periods throttled. A developer hitting their CPU limit feels it as a sluggish IDE."),
            table("Workspace placement", [(f'count by (exported_job, host) (nomad_client_allocs_memory_usage{{{WS_MAIN},'
                                           f'task=~".*-devpod-.*"}})', '{{exported_job}} @ {{host}}')], w=12,
                  desc="Which client runs each workspace (value = task count)."),
        ]),
        ("Access paths", [
            ts("SSH sessions by workspace", [(f'sum by (envoy_cluster_name) (envoy_cluster_upstream_cx_active'
                                              f'{{service="sm-jump-bastion",{WSC}}})', '{{envoy_cluster_name}}')], w=8,
               desc="Active bastion connections per workspace."),
            ts("Web connections by workspace", [(f'sum by (envoy_cluster_name) (envoy_cluster_upstream_cx_active'
                                                 f'{{service="sm-ingress-traefik",{WSC}}})', '{{envoy_cluster_name}}')],
               w=8, desc="Active Traefik connections per workspace."),
            ts("Access throughput", [(f'sum by (service) (rate(envoy_cluster_upstream_cx_rx_bytes_total{{service=~"sm-jump-bastion|'
                                      f'sm-ingress-traefik",{WSC}}}[{RI}]) + rate(envoy_cluster_upstream_cx_tx_bytes_total'
                                      f'{{service=~"sm-jump-bastion|sm-ingress-traefik",{WSC}}}[{RI}]))', '{{service}}')],
               unit="Bps", w=8, desc="Bytes exchanged with workspaces through the bastion and Traefik."),
        ]),
        ("Platform services workspaces depend on", [
            ts("Inbound HTTP by plane", [
                (f'sum by (nomad_namespace) (rate(envoy_http_downstream_rq_total{{{PUB},nomad_namespace=~'
                 f'"remote-dev-ws-onboarding|ory-identity|gitlab|nexus-uar|sm-ingress"}}[{RI}]))', '{{nomad_namespace}}')],
               unit="reqps", w=12, desc="Request load on onboarding, identity, GitLab, Nexus and ingress."),
            ts("5xx by plane", [
                (f'sum by (nomad_namespace) (rate(envoy_http_downstream_rq_xx{{{PUB},envoy_response_code_class="5",'
                 f'nomad_namespace=~"remote-dev-ws-onboarding|ory-identity|gitlab|nexus-uar|sm-ingress"}}[{RI}]))',
                 '{{nomad_namespace}}')], unit="reqps", w=12, desc="Server errors per plane."),
            ts("Onboarding p95 latency", [
                (f'histogram_quantile(0.95, sum by (service, le) (rate(envoy_http_downstream_rq_time_bucket{{{PUB},'
                 f'nomad_namespace="remote-dev-ws-onboarding"}}[{LAT}])))', '{{service}}')], unit="ms", w=12,
               desc="Portal, TUI and Oathkeeper request time."),
            ts("GitLab runners per developer (memory)", [
                ('sum by (task_group) (nomad_client_allocs_memory_usage{namespace="gitlab",exported_job="gitlab-runner"})',
                 '{{task_group}}')], unit="bytes", w=12, desc="Per-developer GitLab runner memory."),
        ]),
    ],
    [query_var("developer", "Developer",
               'label_values(nomad_client_allocs_memory_usage{exported_job=~".+-devpod-.+"}, namespace)'),
     query_var("workspace", "Workspace",
               'label_values(nomad_client_allocs_memory_usage{namespace=~"$developer",exported_job=~".+-devpod-.+"}, exported_job)')])

# ---------------------------------------------------------------- 90 prometheus
P = 'job="prometheus"'
prom = dashboard(
    "hashistack-prometheus", "HashiStack / Prometheus",
    "Self-observability of the gcloud-dc Prometheus: scrape coverage and cost per job, TSDB growth "
    "against the 8GB / 15d retention, rule evaluation and the alert set, process resources.",
    ["prometheus", "meta"],
    [
        ("Status", [
            stat("Targets up", 'count(up == 1)', th=ONE_GOOD, desc="Targets answering."),
            stat("Targets down", 'count(up == 0) or vector(0)', th=ZERO_GOOD, desc="Targets failing their scrape."),
            stat("Watchdog firing", 'count(ALERTS{alertname="Watchdog",alertstate="firing"}) or vector(0)',
                 th=ONE_GOOD, graph=False, desc="The always-firing Watchdog proves the rule engine is evaluating."),
            stat("Config reload OK", f'min(prometheus_config_last_reload_successful{{{P}}})', th=ONE_GOOD, graph=False,
                 desc="0 = the last hot reload failed and Prometheus runs the previous config "
                 "(alert PrometheusConfigReloadFailed)."),
            stat("Rule eval failures (range)", f'sum(increase(prometheus_rule_evaluation_failures_total{{{P}}}[$__range]))',
                 th=ZERO_GOOD, decimals=0, graph=False, desc="Failed rule evaluations in the time range."),
            stat("Head series", f'sum(prometheus_tsdb_head_series{{{P}}})', desc="Active series in the head block."),
            stat("Samples / s", f'sum(rate(prometheus_tsdb_head_samples_appended_total{{{P}}}[{RI}]))', unit="ops",
                 decimals=0, desc="Ingestion rate."),
            stat("Storage vs cap", f'sum(prometheus_tsdb_storage_blocks_bytes{{{P}}}) / '
                 f'sum(prometheus_tsdb_retention_limit_bytes{{{P}}})', unit="percentunit", th=UTIL, decimals=1,
                 desc="Persisted block bytes / the 8GB size retention. At 100% the oldest blocks are deleted early."),
            stat("Retained history", f'time() - min(prometheus_tsdb_lowest_timestamp_seconds{{{P}}})', unit="s",
                 desc="Age of the oldest sample (time retention is 15d)."),
            stat("Resident memory", f'sum(process_resident_memory_bytes{{{P}}})', unit="bytes",
                 th=steps(GREEN, (1024 ** 3, AMBER), (1.4 * 1024 ** 3, RED)),
                 desc="Prometheus RSS. The job's memory_max is 1536 MiB."),
            stat("CPU", f'sum(rate(process_cpu_seconds_total{{{P}}}[{RI}]))', unit="short", decimals=2,
                 desc="Cores used by Prometheus."),
            stat("WAL corruptions", f'sum(prometheus_tsdb_wal_corruptions_total{{{P}}}) + '
                 f'sum(prometheus_tsdb_compactions_failed_total{{{P}}})', th=ZERO_GOOD, graph=False,
                 desc="WAL corruptions plus failed compactions since start."),
        ]),
        ("Scrape targets", [
            bargauge("Targets up by job", [('sum by (job) (up)', '{{job}}')], th=NEUTRAL, w=8,
                     desc="Healthy targets per job. Expected on gcloud-dc: node 11, consul 11, nomad 9, vault 1, "
                     "prometheus 1, envoy 37."),
            empty_ok(table("Targets down", [('up == 0', '{{job}} {{instance}} {{service}}')], th=steps(RED), w=8,
                  desc="Targets failing their last scrape. Empty is healthy.")),
            ts("Scrape duration (max per job)", [('max by (job) (scrape_duration_seconds)', '{{job}}')], unit="s",
               th=steps(GREEN, (5, AMBER), (10, RED)), th_style="dashed", w=8,
               desc="Slowest target per job. The scrape timeout is 10s."),
            ts("Samples per scrape by job", [('sum by (job) (scrape_samples_scraped)', '{{job}}')], w=12,
               desc="Series exposed per job; Envoy dominates."),
            bargauge("Heaviest targets", [('topk(10, scrape_samples_scraped)', '{{job}} {{instance}} {{service}}')],
                     th=NEUTRAL, w=12, desc="Targets exposing the most samples."),
        ]),
        ("TSDB", [
            ts("Head series / chunks", [(f'prometheus_tsdb_head_series{{{P}}}', 'series'),
                                        (f'prometheus_tsdb_head_chunks{{{P}}}', 'chunks')], w=8,
               desc="Head block size."),
            ts("Ingestion and churn", [(f'rate(prometheus_tsdb_head_samples_appended_total{{{P}}}[{RI}])', 'samples/s'),
                                       (f'rate(prometheus_tsdb_head_series_created_total{{{P}}}[{RI}])', 'series created/s')],
               unit="ops", w=8, desc="Sample ingest rate and new-series churn (allocation replacement creates series)."),
            ts("Block storage", [(f'prometheus_tsdb_storage_blocks_bytes{{{P}}}', 'blocks'),
                                 (f'prometheus_tsdb_retention_limit_bytes{{{P}}}', 'size retention')], unit="bytes",
               w=8, desc="Persisted blocks against the size retention."),
            ts("WAL and head chunk storage", [(f'prometheus_tsdb_wal_storage_size_bytes{{{P}}}', 'WAL'),
                                              (f'prometheus_tsdb_head_chunks_storage_size_bytes{{{P}}}', 'head chunks (mmap)')],
               unit="bytes", w=12, desc="On-disk WAL and memory-mapped head chunks. A WAL that keeps growing means "
               "checkpoints or head truncation are failing."),
            ts("Scrape interval accuracy", [(f'prometheus_target_interval_length_seconds{{quantile="0.99",{P}}}', '{{interval}} p99'),
                                            (f'prometheus_target_interval_length_seconds{{quantile="0.5",{P}}}', '{{interval}} p50')],
               unit="s", mn=None, w=12, desc="Actual time between scrapes. Drift above 15s means Prometheus is overloaded."),
        ]),
        ("Rules and alerts", [
            table("Alerts", [('ALERTS', '{{alertname}} ({{alertstate}}, {{severity}}) {{instance}}')], w=12,
                  desc="Every pending or firing alert."),
            ts("Rule group duration vs interval", [
                (f'prometheus_rule_group_last_duration_seconds{{{P}}} / prometheus_rule_group_interval_seconds{{{P}}}',
                 '{{rule_group}}')], unit="percentunit", th=UTIL, w=12,
               desc="Evaluation time as a share of the interval; at 100% groups miss evaluations."),
            ts("Query latency p99", [(f'prometheus_engine_query_duration_seconds{{quantile="0.99",{P}}}', '{{slice}}')],
               unit="s", w=12, desc="PromQL engine time by phase. Dashboards and rules share this engine."),
            ts("HTTP API requests", [(f'topk(8, sum by (handler) (rate(prometheus_http_requests_total{{{P}}}[{RI}])))',
                                      '{{handler}}')], unit="reqps", w=12, desc="API load by handler (grafana-tui uses "
                                      "query_range, labels and series)."),
        ]),
    ])

for fname, d in [("00-overview.json", overview), ("10-nodes.json", nodes), ("20-nomad.json", nomad),
                 ("30-consul.json", consul), ("40-vault.json", vault), ("50-service-mesh.json", mesh),
                 ("60-rdw-workspaces.json", rdw), ("90-prometheus.json", prom)]:
    write(os.path.join(OUT, fname), d)
    print(fname, len(d["spec"]["elements"]), "panels")
