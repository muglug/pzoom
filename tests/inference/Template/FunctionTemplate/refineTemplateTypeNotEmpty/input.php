<?php
/**
 * @template T of Iterator|null
 * @param T $iterator
 */
function toArray($iterator): array
{
    if ($iterator) {
        return iterator_to_array($iterator);
    }

    return [];
}