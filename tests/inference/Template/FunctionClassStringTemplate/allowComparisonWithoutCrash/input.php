<?php
/**
 * @template T as object
 *
 * @param T::class $e
 * @param T::class $expected
 */
function bar(string $e, string $expected) : void {
    if ($e !== $expected) {}
}