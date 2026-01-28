<?php
namespace Psl\Arr;

/**
 * @template Tk of array-key
 * @template Tv
 *
 * @param array<Tk, Tv> $result
 * @param (callable(Tk, Tk): int) $comparator
 *
 * @preturn array<Tk, Tv>
 */
function sort_by_key(array $result, callable $comparator): array
{
    \uksort($result, $comparator);
    return $result;
}