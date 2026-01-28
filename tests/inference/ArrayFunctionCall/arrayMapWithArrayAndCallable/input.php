<?php
/**
 * @psalm-return array<array-key, int>
 */
function foo(array $v): array {
    $r = array_map("intval", $v);
    return $r;
}
