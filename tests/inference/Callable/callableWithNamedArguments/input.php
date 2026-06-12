<?php
/** @param callable(int $i) $c */
function f(callable $c): void {
    $c(i: 1);
}
