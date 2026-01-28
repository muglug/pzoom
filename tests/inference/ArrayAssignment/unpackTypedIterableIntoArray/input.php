<?php

/**
 * @param iterable<int, string> $data
 * @return list<string>
 */
function unpackIterable(iterable $data): array
{
    return [...$data];
}
