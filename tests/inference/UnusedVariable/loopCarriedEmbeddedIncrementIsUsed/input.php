<?php

/** @return array<int, string> */
function fillRange(int $first, int $count, string $value): array
{
    $result = [];
    while ($count > 0) {
        $result[$first++] = $value;
        $count--;
    }
    return $result;
}

/** @return array<int, string> */
function fillRangePrefix(int $first, int $count, string $value): array
{
    $result = [];
    while ($count > 0) {
        $result[++$first] = $value;
        $count--;
    }
    return $result;
}

function deadEmbeddedIncrement(int $first, string $value): array
{
    $result = [];
    $result[$first++] = $value;
    return $result;
}
