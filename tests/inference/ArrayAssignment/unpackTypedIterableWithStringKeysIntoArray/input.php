<?php

/**
 * @param iterable<string, string> $data
 * @return array<string, string>
 */
function unpackIterable(iterable $data): array
{
    return [...$data];
}
