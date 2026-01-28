<?php
/** @psalm-suppress MixedArrayOffset */
function foo(array $a, array $b) : void {
    if (! isset($b["id"], $a[$b["id"]])) {
        echo "z";
    }
}