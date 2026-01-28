<?php
/**
 * @param array{a: string, b: string, c?: string} $info
 */
function getReason(array $info, string $key, string $value): bool {
    if (array_key_exists($key, $info) && $info[$key] === $value) {
        return true;
    }

    return false;
}