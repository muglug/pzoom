<?php
function rm(string $dir): void {
    $full_path = $dir . '/x';
    if (is_dir($full_path)) {
        strlen($full_path);
    } else {
        while (rand(0, 1) > 0) {
            if (rand(0, 1)) {
                break;
            }
        }
        echo $full_path;
    }
}
