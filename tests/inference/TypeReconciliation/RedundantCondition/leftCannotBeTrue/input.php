<?php
/** @psalm-type F = ""|"0" */
/**
 * @param F $a
 * @param F $b
 */
function foo(string $a, string $b): void {
    if ($a || $b) {}
}
