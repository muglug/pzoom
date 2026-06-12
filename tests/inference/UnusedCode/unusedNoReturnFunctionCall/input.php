<?php
/**
 * @return no-return
 *
 * @pure
 *
 * @throws RuntimeException
 */
function invariant_violation(string $message): void
{
    throw new RuntimeException($message);
}

/**
 * @pure
 */
function reverse(string $string): string
{
    if ("" === $string) {
        invariant_violation("i do not like empty strings.");
    }

    return strrev($string);
}
