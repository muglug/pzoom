<?php
function expectsInt(int $a) : void {}

/**
 * @param array<array-key, array{item:int}> $list
 */
function test(array $list) : void
{
    while (!empty($list)) {
        $tmp = array_pop($list);
        if ($tmp === null) {}
    }
}
