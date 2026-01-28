<?php
class C {}
function foo(?C $a, ?C $b): void {
    if ($a && $b) {
        echo "a";
    } elseif ($a && $b) {
        echo "b";
    }
}
