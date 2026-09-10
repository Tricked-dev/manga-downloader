{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.infra.performanceProfiling;
in
{
  config = lib.mkIf cfg.enable {
    environment.systemPackages = [
      pkgs.perf
    ];

    boot = {
      kernelModules = lib.mkAfter [
        "tcp_bbr"
      ];

      kernelParams = lib.mkAfter [
        "transparent_hugepage=madvise"
      ];

      kernel.sysctl = {
        "kernel.kptr_restrict" = lib.mkForce 1;
        "kernel.nmi_watchdog" = 0;
        "kernel.perf_event_paranoid" = lib.mkForce 1;
        "net.core.default_qdisc" = "fq";
        "net.core.netdev_max_backlog" = 16384;
        "net.core.optmem_max" = 16777216;
        "net.core.rmem_max" = 16777216;
        "net.core.somaxconn" = 4096;
        "net.core.wmem_max" = 16777216;
        "net.ipv4.ip_local_port_range" = "16384 65535";
        "net.ipv4.tcp_congestion_control" = "bbr";
        "net.ipv4.tcp_fin_timeout" = 15;
        "net.ipv4.tcp_max_syn_backlog" = 8192;
        "net.ipv4.tcp_mtu_probing" = 1;
        "net.ipv4.tcp_rmem" = "4096 87380 16777216";
        "net.ipv4.tcp_slow_start_after_idle" = 0;
        "net.ipv4.tcp_wmem" = "4096 65536 16777216";
        "vm.dirty_background_ratio" = 5;
        "vm.dirty_ratio" = 15;
        "vm.swappiness" = 10;
        "vm.vfs_cache_pressure" = 50;
      };
    };
  };
}
