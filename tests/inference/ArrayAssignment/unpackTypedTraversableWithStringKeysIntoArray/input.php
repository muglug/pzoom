<?php

/**
 * @param Traversable<string, string> $data
 * @return array<string, string>
 */
function unpackIterable(Traversable $data): array
{
    return [...$data];
}
