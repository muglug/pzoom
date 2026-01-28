<?php
function test(int $x = null): int {
    if (!$x && !($x = rand(0, 10))) {
        echo "Failed to get non-empty x\n";
        return -1;
    }
    return $x;
}