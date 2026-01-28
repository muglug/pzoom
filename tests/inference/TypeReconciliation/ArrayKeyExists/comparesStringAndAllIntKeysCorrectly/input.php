<?php
/**
 * @param array<1|2|3, string> $arr
 * @return bool
 */
function checkArrayKeyExistsComparison(array $arr, string $key): bool
{
    if (array_key_exists($key, $arr)) {
        return true;
    }
    return false;
}