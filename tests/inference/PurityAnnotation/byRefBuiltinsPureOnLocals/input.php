<?php
/**
 * @psalm-pure
 * @param non-empty-list<int> $a
 * @param non-empty-list<int> $b
 */
function f(array $a, array $b): bool {
    $last = array_pop($a);
    return reset($b) === $last;
}
