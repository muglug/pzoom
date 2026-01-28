<?php
/**
 * @return array<int, mixed>
 * @psalm-suppress MixedAssignment
 */
function fetchFromCache(mixed $m)
{
    $data = [];

    try {
        $value = $m;
    } catch (Throwable $e) {
        $value = $m;
    }

    $data[] = $value;

    return $data;
}
