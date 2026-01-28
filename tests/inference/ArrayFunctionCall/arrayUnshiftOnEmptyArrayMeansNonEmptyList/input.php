<?php
/**
 * @return non-empty-list<string>
 */
function foo(): array
{
    $a = [];

    array_unshift($a, "string");

    return $a;
}
