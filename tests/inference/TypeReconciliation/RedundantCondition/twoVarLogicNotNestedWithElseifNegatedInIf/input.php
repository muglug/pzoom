<?php
function foo(?string $a, ?string $b): ?string {
    if ($a) {
        $a = null;
    } elseif ($b) {
        // do nothing here
    } else {
        return "bad";
    }

    if (!$a) return $b;
    return $a;
}
