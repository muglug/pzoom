<?php
function foo(?string $a, ?string $b): string {
    if ($a) {
        $a = "";
    } elseif ($b) {
        // do nothing
    } else {
        return "bad";
    }

    if (!$a) return $b;
    return $a;
}
