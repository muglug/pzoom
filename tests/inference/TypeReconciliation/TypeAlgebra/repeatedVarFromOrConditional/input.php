<?php
function foo(string $a, string $b): void {
    if ($a || $b) {
        echo "a";
    } elseif ($a) {
        echo "b";
    }
}
