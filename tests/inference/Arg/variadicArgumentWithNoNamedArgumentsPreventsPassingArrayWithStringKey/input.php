<?php
/**
 * @no-named-arguments
 * @psalm-return list<int>
 */
function foo(int ...$values): array
{
    return $values;
}

foo(...["a" => 0]);
