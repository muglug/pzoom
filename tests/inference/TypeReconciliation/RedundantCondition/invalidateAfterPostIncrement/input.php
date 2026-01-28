<?php
/**
 * @param array<int, int> $tokens
 */
function propertyInUse(array $tokens, int $i): bool {
    if ($tokens[$i] !== 1) {
        return false;
    }
    $i++;
    if ($tokens[$i] !== 2) {}
    return false;
}