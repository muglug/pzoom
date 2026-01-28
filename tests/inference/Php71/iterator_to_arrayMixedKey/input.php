<?php
/**
 * @template TKey
 * @template TValue
 * @param Traversable<TKey, TValue> $traversable
 * @return array<TValue>
 */
function toArray(Traversable $traversable): array
{
    return iterator_to_array($traversable);
}
