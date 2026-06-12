<?php

/**
 * @psalm-assert-if-false !numeric $literal_array_key
 */
function getLiteralArrayKeyInt(int|string $literal_array_key): int|false
{
    if (is_numeric($literal_array_key)) {
        return (int) $literal_array_key;
    }
    return false;
}

/** @param int|numeric-string $k */
function wantNumeric($k): bool
{
    return $k !== 0;
}

/**
 * @param int|string $key
 */
function adjustKey($key): bool
{
    if (getLiteralArrayKeyInt($key) === false
        || ($key && wantNumeric($key))
    ) {
        return true;
    }
    return false;
}
