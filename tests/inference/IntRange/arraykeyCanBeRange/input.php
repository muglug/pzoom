<?php
/**
 * @param array-key $key
 * @param positive-int|non-negative-int|negative-int|non-positive-int $expected
 */
function matches($key, int $expected): bool {
    if ($key !== $expected) {
        return false;
    }

    return true;
}
