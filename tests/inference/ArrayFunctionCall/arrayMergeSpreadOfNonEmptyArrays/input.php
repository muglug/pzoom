<?php
abstract class Atomic22 {}

/**
 * @param list<non-empty-array<array-key, Atomic22>> $types
 * @return non-empty-array<array-key, Atomic22>|null
 */
function resolve(array $types): ?array {
    if ($types === []) {
        return null;
    }
    $merged = array_merge([], ...$types);
    return $merged;
}
