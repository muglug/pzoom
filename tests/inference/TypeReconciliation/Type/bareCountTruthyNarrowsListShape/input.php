<?php
/** @param list<string> $args */
function f(array $args): ?string {
    $init_source_dir = null;
    if (count($args)) {
        if (count($args) > 2) {
            exit(1);
        }
        if (isset($args[1])) {
            if (!preg_match('/^[1-8]$/', $args[1])) {
                exit(1);
            }
            echo (int) $args[1];
        }
        $init_source_dir = $args[0];
    }
    return $init_source_dir;
}
