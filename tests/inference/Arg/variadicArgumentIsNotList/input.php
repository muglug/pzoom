<?php
/** @psalm-return list<int> */
function foo(int ...$values): array
{
    return $values;
}
