<?php

/**
 * @param Traversable<int, string> $data
 * @return list<string>
 */
function unpackIterable(Traversable $data): array
{
    return [...$data];
}
