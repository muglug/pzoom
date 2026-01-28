<?php

/**
 * @template T
 */
final class Valid {}
final class Invalid {}

/**
 * @template T
 *
 * @param Valid<T>|Invalid $val
 * @psalm-assert-if-true Valid<T> $val
 */
function isValid($val): bool
{
    return $val instanceof Valid;
}

/**
 * @template T
 * @param Valid<T>|Invalid $val
 */
function genericContext($val): void
{
    $takesValid =
        /** @param Valid<T> $_valid */
        function ($_valid): void {};

    $takesInvalid =
        /** @param Invalid $_invalid */
        function ($_invalid): void {};

    isValid($val) ? $takesValid($val) : $takesInvalid($val);
}
