<?php
/**
 * @template TKey
 * @template TValue
 * @param iterable<TKey, TValue> $iterable
 * @return array<TKey, TValue>
 */
function toArray(iterable $iterable): array
{
    return iterator_to_array($iterable);
}
