<?php
/**
 * @return array<int, array{string,?string}>
 */
function getCharPairs(string $line) : array {
    $chars = str_split($line);
    return array_map(
        null,
        $chars,
        array_slice($chars, 1)
    );
}
