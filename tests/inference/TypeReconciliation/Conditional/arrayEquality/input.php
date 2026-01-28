<?php
/**
 * @param array<string, array<array-key, string|int>> $haystack
 * @param array<array-key, int|string> $needle
 */
function foo(array $haystack, array $needle) : void {
    foreach ($haystack as $arr) {
        if ($arr === $needle) {}
    }
}