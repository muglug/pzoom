<?php
/** @param callable(int $a) $c */
function f(callable $c): void {
    $c(b: 1);
}
