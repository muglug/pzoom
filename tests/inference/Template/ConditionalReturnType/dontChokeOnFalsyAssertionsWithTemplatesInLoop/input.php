<?php
/**
 * @psalm-return ($list_output is true ? list : array)
 */
function scope(bool $list_output = true): array
{
    for ($i = 0; $i < 5; $i++) {
        $list_output ? [] : [];
    }

    return [];
}