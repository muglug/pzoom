<?php
/**
 * @param array{A: int} $a
 * @param array<string, string> $b
 *
 * @return array{A: int, ...}
 */
function merger(array $a, array $b) : array {
    return array_merge($b, $a);
}
