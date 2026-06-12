<?php
/**
 * @return non-empty-array
 */
function test(array $array): array
{
    return match (\count($array)) {
        0 => throw new \InvalidArgumentException,
        default => $array,
    };
}
