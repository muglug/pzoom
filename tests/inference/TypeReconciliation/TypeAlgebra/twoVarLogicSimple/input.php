<?php
function foo(?string $a, ?string $b): string {
    if ($a !== null || $b !== null) {
        if ($a !== null) {
            return $a;
        } else {
            return $b;
        }
    }

    return "foo";
}
