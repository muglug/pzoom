<?php
function foo(?string $a, ?string $b): string {
    if (rand(0, 1)) {
        // do nothing
    } elseif ($a || $b) {
        // do nothing here
    } else {
        return "bad";
    }

    if (!$a) return $b;
    return $a;
}
