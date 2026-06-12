<?php

proc_open(
    command: "ls",
    descriptor_spec: [],
    pipes: $pipes,
    cwd: null,
    env_vars: null,
    options: null
);
