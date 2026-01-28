<?php
/**
 * @param array<1|2|3, string> $arr
 * @return 1|2|3
 */
function checkArrayKeyExistsInt(array $arr, int $int): int
{
    if (array_key_exists($int, $arr)) {
        return $int;
    }

    return 1;
}