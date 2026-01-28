<?php
/**
 * @psalm-param iterable<string> $collection
 * @psalm-return array<string>
 */
function order(iterable $collection): array {
    if ($collection instanceof \Traversable) {
        $collection = iterator_to_array($collection, false);
    }

    return $collection;
}