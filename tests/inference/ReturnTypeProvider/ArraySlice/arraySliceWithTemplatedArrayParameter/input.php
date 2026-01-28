<?php
/**
 * @template T as string[]
 * @param T $a
 * @return string[]
 */
function f(array $a): array
{
    return array_slice($a, 1);
}
            
