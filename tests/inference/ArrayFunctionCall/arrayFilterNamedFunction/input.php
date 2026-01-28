<?php
/**
 * @param array<int, DateTimeImmutable|null> $a
 * @return array<int, DateTimeImmutable>
 */
function foo(array $a) : array {
    return array_filter($a, "is_object");
}
