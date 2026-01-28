<?php
function foo(?string $a, ?string $b): string {
    if ($a !== null || $b !== null) {
        $b = null;
    } else {
        return "bad";
    }

    if ($a !== null) return $b;
    return $a;
}
