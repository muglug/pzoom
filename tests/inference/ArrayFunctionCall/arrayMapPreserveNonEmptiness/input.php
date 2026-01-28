<?php
/**
 * @psalm-param non-empty-list<string> $strings
 * @psalm-return non-empty-list<int>
 */
function foo(array $strings): array {
    return array_map("intval", $strings);
}
