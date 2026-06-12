<?php
/**
 * @return array<string, mixed>
 */
function foo(int $a, string $b, bool $c) : array {
    return compact("a", "b", "c");
}
