{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.infra;
  deployCfg = cfg.deployment.githubActions;
  selinuxPolicy = pkgs.selinux-refpolicy.override {
    checkpolicy = pkgs.buildPackages.checkpolicy;
    policycoreutils = pkgs.buildPackages.policycoreutils;
    semodule-utils = pkgs.buildPackages.semodule-utils;
  };
  corePackageNames = [
    "bashInteractive"
    "coreutils"
    "curl"
    "findutils"
    "gnugrep"
    "gnused"
    "procps"
    "util-linux"
  ];
  ghosttyTerminfo =
    pkgs.runCommand "ghostty-terminfo-lite" { nativeBuildInputs = [ pkgs.buildPackages.ncurses ]; }
      ''
            cat > ghostty.terminfo <<'EOF'
        ghostty|Ghostty,
          use=xterm-256color,
        xterm-ghostty|Ghostty,
          use=xterm-256color,
        EOF
            mkdir -p "$out/share/terminfo"
            tic -x -o "$out/share/terminfo" ghostty.terminfo
      '';
  lowerPriority = pkg: lib.setPrio ((pkg.meta.priority or lib.meta.defaultPriority) + 3) pkg;
in
{
  config = {
    time.timeZone = cfg.timezone;

    nix.settings = {
      experimental-features = [
        "nix-command"
        "flakes"
      ];
      auto-optimise-store = true;
      sandbox = true;
      allowed-users = [
        "@wheel"
      ];
      trusted-users = [
        "root"
        "@wheel"
      ];
      warn-dirty = false;
    };

    environment.systemPackages = with pkgs; [
      ghosttyTerminfo
      htop
      policycoreutils
      selinuxPolicy
    ];

    environment.etc = {
      "issue".text = ''
        Authorized access only. Activity may be monitored.
      '';
      "issue.net".text = ''
        Authorized access only. Activity may be monitored.
      '';
      "selinux/config".text = ''
        SELINUX=enforcing
        SELINUXTYPE=refpolicy
      '';
      "selinux/refpolicy".source = "${selinuxPolicy}/etc/selinux/refpolicy";
    };

    environment.corePackages = lib.mkForce (
      (map (name: lowerPriority pkgs.${name}) corePackageNames)
      ++ [
        pkgs.stdenv.cc.libc
      ]
    );

    boot = {
      kernel.sysctl = {
        "dev.tty.ldisc_autoload" = 0;
        "fs.protected_fifos" = 2;
        "fs.protected_hardlinks" = 1;
        "fs.protected_regular" = 2;
        "fs.protected_symlinks" = 1;
        "fs.suid_dumpable" = 0;
        "kernel.core_uses_pid" = 1;
        "kernel.dmesg_restrict" = 1;
        "kernel.kptr_restrict" = 2;
        "kernel.oops_limit" = 10;
        "kernel.perf_event_paranoid" = 3;
        "kernel.randomize_va_space" = 2;
        "kernel.sysrq" = 0;
        "kernel.unprivileged_bpf_disabled" = 1;
        "net.core.bpf_jit_harden" = 2;
        "net.ipv4.conf.all.accept_redirects" = 0;
        "net.ipv4.conf.all.accept_source_route" = 0;
        "net.ipv4.conf.all.log_martians" = 1;
        "net.ipv4.conf.all.rp_filter" = 1;
        "net.ipv4.conf.all.secure_redirects" = 0;
        "net.ipv4.conf.all.send_redirects" = 0;
        "net.ipv4.conf.all.shared_media" = 0;
        "net.ipv4.conf.default.accept_redirects" = 0;
        "net.ipv4.conf.default.accept_source_route" = 0;
        "net.ipv4.conf.default.log_martians" = 1;
        "net.ipv4.conf.default.rp_filter" = 1;
        "net.ipv4.conf.default.secure_redirects" = 0;
        "net.ipv4.conf.default.send_redirects" = 0;
        "net.ipv4.conf.default.shared_media" = 0;
        "net.ipv4.icmp_echo_ignore_broadcasts" = 1;
        "net.ipv4.icmp_ignore_bogus_error_responses" = 1;
        "net.ipv4.tcp_rfc1337" = 1;
        "net.ipv4.tcp_syncookies" = 1;
        "net.ipv6.conf.all.accept_redirects" = 0;
        "net.ipv6.conf.all.accept_source_route" = 0;
        "net.ipv6.conf.default.accept_redirects" = 0;
        "net.ipv6.conf.default.accept_source_route" = 0;
      };

      kernelParams = [
        "selinux=1"
        "enforcing=1"
        "init_on_alloc=1"
        "init_on_free=1"
        "page_alloc.shuffle=1"
        "preempt=none"
        "slab_nomerge"
        "vsyscall=none"
      ];

      kernelModules = lib.mkAfter [
        "fuse"
        "overlay"
        "tun"
      ];

      blacklistedKernelModules = [
        # Rare network protocol families. This host needs ordinary TCP/UDP,
        # Tailscale's tun device, and container overlay/fuse support, but not
        # legacy or special-purpose protocol stacks that can be autoloaded by
        # unprivileged socket users.
        "af_802154"
        "appletalk"
        "atm"
        "ax25"
        "can"
        "dccp"
        "decnet"
        "econet"
        "ipx"
        "n-hdlc"
        "netrom"
        "rds"
        "rose"
        "sctp"
        "tipc"
        "x25"

        # VM sockets are not used by this guest's management path. Keep the
        # virtio block/network/console stack available through qemu-guest.
        "vsock"
        "vmw_vsock_virtio_transport"
        "vmw_vsock_vmci_transport"

        # Peripheral and workstation hardware absent from the Netcup VPS.
        "batman-adv"
        "bluetooth"
        "bnep"
        "btusb"
        "cfg80211"
        "firewire-core"
        "firewire-ohci"
        "floppy"
        "hidp"
        "irda"
        "joydev"
        "mac80211"
        "nfc"
        "pcspkr"
        "rfcomm"
        "snd"
        "snd_hda_intel"
        "snd_pcm"
        "thunderbolt"
        "usb-storage"
        "uvcvideo"
        "vivid"

        # Filesystems absent from the image runtime. Keep btrfs, squashfs, and
        # vfat available for /var, /nix/store, and /boot.
        "adfs"
        "affs"
        "befs"
        "bfs"
        "ceph"
        "cifs"
        "cramfs"
        "efs"
        "ext4"
        "f2fs"
        "freevxfs"
        "gfs2"
        "hfs"
        "hfsplus"
        "hpfs"
        "isofs"
        "jffs2"
        "jfs"
        "ksmbd"
        "minix"
        "nfs"
        "nfsv3"
        "nfsv4"
        "nilfs2"
        "ntfs3"
        "omfs"
        "qnx4"
        "qnx6"
        "udf"
        "ufs"
        "virtiofs"
      ];

      tmp.cleanOnBoot = true;
    };

    security = {
      apparmor.enable = false;
      lsm = [ "selinux" ];
      audit.enable = true;
      auditd.enable = true;
      loginDefs.settings = {
        PASS_MAX_DAYS = 365;
        PASS_MIN_DAYS = 1;
        PASS_WARN_AGE = 14;
        SHA_CRYPT_MAX_ROUNDS = 200000;
        SHA_CRYPT_MIN_ROUNDS = 100000;
        YESCRYPT_COST_FACTOR = 11;
      };
      lockKernelModules = true;
      protectKernelImage = true;
      sudo.enable = false;
      sudo-rs.enable = false;
      polkit.enable = true;
      run0 = {
        enableSudoAlias = true;
        wheelNeedsPassword = !cfg.bootstrap.directSsh;
      };
    };

    systemd.package = pkgs.systemd.override {
      withSelinux = true;
    };

    users = {
      groups = lib.optionalAttrs deployCfg.enable {
        ${deployCfg.user} = { };
      };

      users = {
        samuel = {
          isNormalUser = true;
          description = "Samuel";
          extraGroups = [
            "wheel"
          ];
          openssh.authorizedKeys.keys = [
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOIGWJvoRJR+vOS8gEObnlv1znuopr5V6XfFg3kxySUX trashcan69666@gmail.com"
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICV7yX3MxvfXpDR3xYiw2cTSEhNG2PRkdXqbGRcx/87u samuel@Samuels-MacBook-Pro.local"
          ];
        };

        root = {
          openssh.authorizedKeys.keys = config.users.users.samuel.openssh.authorizedKeys.keys;
        };
      }
      // lib.optionalAttrs deployCfg.enable {
        ${deployCfg.user} = {
          isNormalUser = true;
          description = "GitHub Actions deploy user";
          group = deployCfg.user;
          home = "/home/${deployCfg.user}";
          createHome = true;
          extraGroups = [
            "wheel"
          ];
          openssh.authorizedKeys.keys = deployCfg.authorizedKeys;
        };
      };
    };

    services.openssh = {
      enable = true;
      openFirewall = cfg.bootstrap.directSsh;
      settings = {
        AllowUsers = [
          "samuel"
          "root"
        ]
        ++ lib.optional deployCfg.enable deployCfg.user;
        AuthenticationMethods = "publickey";
        ClientAliveCountMax = 2;
        ClientAliveInterval = 300;
        KbdInteractiveAuthentication = false;
        LoginGraceTime = 30;
        MaxAuthTries = 3;
        MaxSessions = 2;
        MaxStartups = "2:30:10";
        Banner = "/etc/issue.net";
        PermitEmptyPasswords = false;
        PermitTunnel = false;
        PermitUserEnvironment = false;
        PermitRootLogin = "prohibit-password";
        PasswordAuthentication = false;
        PubkeyAuthentication = true;
        StrictModes = !cfg.bootstrap.directSsh;
        X11Forwarding = false;
        AllowAgentForwarding = false;
        AllowStreamLocalForwarding = "no";
        AllowTcpForwarding = "remote";
        PermitListen = "127.0.0.1:18888";
      };
    };

    security.pam.services.sshd.startSession = lib.mkForce false;

    services.logind.settings.Login = {
      NAutoVTs = 0;
      ReserveVT = 0;
    };
    services.logind.enable = lib.mkForce false;

    services.nscd.enable = false;
    systemd.oomd.enable = lib.mkForce false;
    system.nssModules = lib.mkForce [ ];

    systemd.enableEmergencyMode = false;
    systemd.sockets.dbus.wantedBy = [ "sockets.target" ];
    systemd.suppressedSystemUnits = [
      "autovt@.service"
      "autovt@tty1.service"
      "emergency.service"
      "emergency.target"
      "getty@.service"
      "getty@tty1.service"
      "reload-systemd-vconsole-setup.service"
      "rescue.service"
      "rescue.target"
      "serial-getty@.service"
      "serial-getty@ttyAMA0.service"
      "systemd-ask-password-console.path"
      "systemd-ask-password-console.service"
      "systemd-ask-password-wall.path"
      "systemd-ask-password-wall.service"
      "systemd-hostnamed.service"
      "systemd-hostnamed.socket"
      "systemd-logind.service"
      "systemd-oomd.service"
      "systemd-oomd.socket"
      "systemd-timedated.service"
    ];

    systemd.services = {
      "getty@tty1".enable = false;
      "serial-getty@ttyAMA0".enable = false;

      auditd.serviceConfig = {
        AmbientCapabilities = "";
        CapabilityBoundingSet = [
          "CAP_AUDIT_CONTROL"
          "CAP_AUDIT_READ"
          "CAP_AUDIT_WRITE"
          "CAP_DAC_READ_SEARCH"
          "CAP_SYS_NICE"
        ];
        IPAddressDeny = "any";
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateMounts = true;
        PrivateNetwork = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        ProcSubset = "pid";
        ReadWritePaths = [
          "/run/audit"
          "/var/log/audit"
        ];
        RestrictAddressFamilies = [
          "AF_NETLINK"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = "~@clock @cpu-emulation @debug @module @mount @obsolete @raw-io @reboot @swap";
        UMask = "0077";
      };

      btrfs-scrub-var.serviceConfig = {
        AmbientCapabilities = "";
        CapabilityBoundingSet = [
          "CAP_DAC_READ_SEARCH"
          "CAP_SYS_ADMIN"
        ];
        IPAddressDeny = "any";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateMounts = true;
        PrivateNetwork = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        ProcSubset = "pid";
        ReadWritePaths = [ "/var" ];
        RestrictAddressFamilies = [ "AF_UNIX" ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = "~@clock @cpu-emulation @debug @module @mount @obsolete @raw-io @reboot @swap";
        UMask = "0077";
      };

      dbus-broker.serviceConfig = {
        AmbientCapabilities = "";
        CapabilityBoundingSet = [
          "CAP_AUDIT_WRITE"
          "CAP_SETGID"
          "CAP_SETUID"
        ];
        IPAddressDeny = "any";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateNetwork = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        RestrictAddressFamilies = [
          "AF_NETLINK"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = "~@clock @cpu-emulation @debug @module @mount @obsolete @raw-io @reboot @swap";
        UMask = "0077";
      };

      qemu-guest-agent.serviceConfig = {
        AmbientCapabilities = "";
        CapabilityBoundingSet = "";
        DeviceAllow = [ "/dev/virtio-ports/org.qemu.guest_agent.0 rw" ];
        DevicePolicy = "closed";
        IPAddressDeny = "any";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateMounts = true;
        PrivateNetwork = true;
        PrivateTmp = true;
        PrivateUsers = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        ProcSubset = "pid";
        ReadWritePaths = [ "/run/qemu-ga" ];
        RemoveIPC = true;
        RestrictAddressFamilies = [ "AF_UNIX" ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = "~@clock @cpu-emulation @debug @module @mount @obsolete @raw-io @reboot @resources @swap";
        UMask = "0077";
      };

      sshd.serviceConfig = {
        CapabilityBoundingSet = [
          "CAP_CHOWN"
          "CAP_DAC_OVERRIDE"
          "CAP_FOWNER"
          "CAP_SETGID"
          "CAP_SETUID"
          "CAP_NET_BIND_SERVICE"
          "CAP_SYS_CHROOT"
        ];
        IPAddressAllow = [
          "127.0.0.0/8"
          "::1/128"
          "100.64.0.0/10"
          "fd7a:115c:a1e0::/48"
        ];
        IPAddressDeny = "any";
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        ReadWritePaths = [
          "/run"
          "/var/log"
        ];
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = "~@clock @cpu-emulation @debug @module @obsolete @raw-io @reboot @resources @swap";
        UMask = "0077";
      };

      systemd-journald.serviceConfig = {
        CapabilityBoundingSet = [
          ""
          "CAP_AUDIT_CONTROL"
          "CAP_AUDIT_READ"
          "CAP_CHOWN"
          "CAP_DAC_OVERRIDE"
          "CAP_DAC_READ_SEARCH"
          "CAP_FOWNER"
          "CAP_SETGID"
          "CAP_SETUID"
          "CAP_SYSLOG"
        ];
        DeviceAllow = [
          ""
          "/dev/kmsg rw"
        ];
        DevicePolicy = "closed";
        PrivateNetwork = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        RestrictAddressFamilies = [
          ""
          "AF_NETLINK"
          "AF_UNIX"
        ];
        UMask = "0077";
      };
      systemd-journald.restartIfChanged = false;

      systemd-journal-flush.serviceConfig = {
        Restart = "on-failure";
        RestartSec = "2s";
      };

      systemd-sysupdate.serviceConfig = {
        BindPaths = [
          "/dev/vda"
          "/dev/vda1"
          "/dev/vda2"
          "/dev/vda3"
          "/dev/vda4"
        ];
        CapabilityBoundingSet = [
          ""
          "CAP_SYS_ADMIN"
        ];
        DeviceAllow = [
          ""
          "/dev/vda rw"
          "/dev/vda1 rw"
          "/dev/vda2 rw"
          "/dev/vda3 rw"
          "/dev/vda4 rw"
        ];
        DevicePolicy = "closed";
        IPAddressDeny = "any";
        PrivateDevices = true;
        PrivateNetwork = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectSystem = "strict";
        ProtectProc = "invisible";
        ProcSubset = "pid";
        ReadWritePaths = [
          "/boot"
          "/var/updates"
        ];
        RestrictAddressFamilies = [
          ""
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictSUIDSGID = true;
        SystemCallFilter = [
          ""
          "@system-service @mount"
          "~@resources"
        ];
        UMask = "0077";
      };

      systemd-networkd.serviceConfig = {
        AmbientCapabilities = [
          ""
          "CAP_NET_ADMIN"
        ];
        CapabilityBoundingSet = [
          ""
          "CAP_NET_ADMIN"
        ];
        DevicePolicy = "closed";
        IPAddressDeny = "any";
        PrivateDevices = true;
        ProtectHostname = true;
        ProtectKernelTunables = true;
        RemoveIPC = true;
        SystemCallFilter = "~@resources";
        UMask = "0077";
      };

      systemd-resolved.serviceConfig = {
        AmbientCapabilities = [
          ""
          "CAP_NET_BIND_SERVICE"
        ];
        CapabilityBoundingSet = [
          ""
          "CAP_NET_BIND_SERVICE"
        ];
        IPAddressAllow = [
          "127.0.0.0/8"
          "::1/128"
          "1.1.1.1/32"
          "1.0.0.1/32"
          "2606:4700:4700::1111/128"
          "2606:4700:4700::1001/128"
        ];
        IPAddressDeny = "any";
        ProtectHostname = true;
        RemoveIPC = true;
        SystemCallFilter = [
          "~@resources"
          "~@privileged"
        ];
        UMask = "0077";
      };

      systemd-udevd.serviceConfig = {
        CapabilityBoundingSet = [
          ""
          "CAP_CHOWN"
          "CAP_DAC_OVERRIDE"
          "CAP_DAC_READ_SEARCH"
          "CAP_FOWNER"
          "CAP_FSETID"
          "CAP_MKNOD"
          "CAP_NET_ADMIN"
          "CAP_SETFCAP"
        ];
        NoNewPrivileges = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        RestrictAddressFamilies = [
          ""
          "AF_NETLINK"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        SystemCallFilter = [
          "~@resources"
        ];
        UMask = "0077";
      };
    };

    systemd.coredump.settings.Coredump = {
      ProcessSizeMax = "0";
      Storage = "none";
    };

    services.timesyncd.enable = lib.mkForce false;

    networking = {
      firewall = {
        enable = true;
        allowPing = false;
        allowedTCPPorts = lib.optionals cfg.bootstrap.directSsh [ 22 ];
        allowedUDPPorts = [ ];
        checkReversePath = "loose";
      };

      nameservers = [
        "1.1.1.1"
        "1.0.0.1"
        "2606:4700:4700::1111"
        "2606:4700:4700::1001"
      ];
    };
  };
}
