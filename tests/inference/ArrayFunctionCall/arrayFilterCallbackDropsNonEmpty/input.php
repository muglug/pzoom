<?php
/**
 * @param non-empty-array<string, int> $types
 * @return list<string>
 */
function f(array $types): array {
    $relevant = array_filter($types, static fn(int $t): bool => $t > 5);
    $names = array_map(static fn(int $t): string => (string) $t, $relevant);
    if (empty($names)) {
        return ['none'];
    }
    return array_values($names);
}
