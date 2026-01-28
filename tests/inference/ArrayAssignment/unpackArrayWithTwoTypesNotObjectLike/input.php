<?php
function int(): int
{
    return 0;
}

/**
 * @return list<positive-int>
 */
function posiviteIntegers(): array
{
    return [1];
}

$_a = [...posiviteIntegers(), int()];
/** @psalm-check-type $_a = non-empty-list<int> */
                
