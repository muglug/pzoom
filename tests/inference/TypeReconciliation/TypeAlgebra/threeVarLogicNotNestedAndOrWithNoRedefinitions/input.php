<?php
function foo(?string $a, ?string $b, ?string $c): string {
    if ($a) {
        // do nothing
    } elseif ($b || $c) {
        // do nothing here
    } else {
        return "bad";
    }

    if (!$a && !$b) return $c;
    if (!$a) return $b;
    return $a;
}