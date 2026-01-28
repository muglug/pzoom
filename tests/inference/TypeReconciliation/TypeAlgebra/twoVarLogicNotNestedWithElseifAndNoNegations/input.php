<?php
function foo(?string $a, ?string $b): string {
    if ($a) {
        // do nothing
    } elseif ($b) {
        // do nothing here
    } else {
        return "bad";
    }

    if (!$a) return $b;
    return $a;
}