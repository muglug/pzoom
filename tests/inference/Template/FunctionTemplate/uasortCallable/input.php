<?php
/**
 * @template T of object
 * @psalm-param array<T> $collection
 * @psalm-param callable(T, T): int $sorter
 * @psalm-return array<T>
 */
function order(array $collection, callable $sorter): array {
    usort($collection, $sorter);

    return $collection;
}