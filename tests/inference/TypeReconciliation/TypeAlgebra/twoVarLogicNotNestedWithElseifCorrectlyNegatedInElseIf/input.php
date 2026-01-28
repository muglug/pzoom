<?php
function foo(string $a, string $b): string {
    if ($a) {
        // do nothing here
    } elseif ($b) {
        $a = null;
    } else {
        return "bad";
    }

    if (!$a) return $b;
    return $a;
}