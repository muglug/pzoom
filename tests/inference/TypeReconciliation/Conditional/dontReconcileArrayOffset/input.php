<?php
/** @psalm-suppress TypeDoesNotContainType */
function foo(array $a) : void {
    if (!is_array($a)) {
        return;
    }

    if ($a[0] === 5) {}
}