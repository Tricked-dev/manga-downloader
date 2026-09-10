#!/usr/bin/env nu

let script = ($env.FILE_PWD | path join "sysupdate-lifecycle.sh")
bash $script bundle
