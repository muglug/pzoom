<?php

/**
 * @param array<array-key, mixed> $data
 * @return array<array-key, mixed>
 */
function unpackArray(array $data): array
{
    return [...$data];
}
