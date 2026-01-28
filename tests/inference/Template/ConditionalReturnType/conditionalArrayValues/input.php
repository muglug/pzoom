<?php
/**
 * @template TValue
 * @template TIterable of ?iterable<TValue>
 * @param TIterable $iterable
 * @return (TIterable is null ? null : list<TValue>)
 */
function toList(?iterable $iterable): ?array {
    if (null === $iterable) {
        return null;
    }

    if (is_array($iterable)) {
        return array_values($iterable);
    }

    return iterator_to_array($iterable, false);
}