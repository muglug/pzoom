<?php
/**
 * @psalm-assert iterable<string> $value
 *
 * @param mixed  $value
 *
 * @throws InvalidArgumentException
 */
function allString($value): void {}

function takesAnArray(array $a): void {
    $keys = array_keys($a);
    allString($keys);
}
