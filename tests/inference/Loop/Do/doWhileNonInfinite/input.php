<?php
function foo(): int {
    do {
        $value = mt_rand(0, 10);
        if ($value > 5) continue;
        break;
    } while (true);

    return $value;
}
