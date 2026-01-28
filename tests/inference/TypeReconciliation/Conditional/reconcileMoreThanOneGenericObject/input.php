<?php

final class Invalid {}

/**
 * @template T
 */
final class Valid {}

/**
 * @template T
 *
 * @param Invalid|Valid<T> $val
 * @psalm-assert-if-true Valid<T> $val
 */
function isValid($val): bool
{
    return $val instanceof Valid;
}

/**
 * @template T
 * @param Valid<T>|Invalid $val1
 * @param Valid<T>|Invalid $val2
 * @param Valid<T>|Invalid $val3
 */
function inGenericContext($val1, $val2, $val3): void
{
    $takesValid =
         /** @param Valid<T> $_valid */
         function ($_valid): void {};

    if (isValid($val1) && isValid($val2) && isValid($val3)) {
        $takesValid($val1);
        $takesValid($val2);
        $takesValid($val3);
    }
}
