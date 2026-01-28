<?php
/**
 * @template I of bool|string|int|stdClass
 * @param I $foo
 */
function bar($foo): void {
    if (is_string($foo)) {}
    if (!is_string($foo)) {}
    if (is_int($foo)) {}
    if (!is_int($foo)) {}
    if (is_numeric($foo)) {}
    if (!is_numeric($foo)) {}
    if (is_scalar($foo)) {}
    if (!is_scalar($foo)) {}
    if (is_bool($foo)) {}
    if (!is_bool($foo)) {}
    if (is_object($foo)) {}
    if (!is_object($foo)) {}
    if (is_callable($foo)) {}
    if (!is_callable($foo)) {}
}
