<?php
/**
 * @return SplFixedArray<string>
 */
function getStrings(): SplFixedArray
{
    return SplFixedArray::fromArray(["fst", "snd", "thr"]);
}
/**
 * @return SplFixedArray<int>
 */
function getIntegers(): SplFixedArray
{
    return SplFixedArray::fromArray([1, 2, 3]);
}
/**
 * @template K
 * @template A
 * @template B
 * @param iterable<K, A> $lhs
 * @param iterable<K, B> $rhs
 * @return iterable<K, A|B>
 */
function mergeIterable(iterable $lhs, iterable $rhs): iterable
{
    foreach ($lhs as $k => $v) { yield $k => $v; }
    foreach ($rhs as $k => $v) { yield $k => $v; }
}
$iterable = mergeIterable(getStrings(), getIntegers());
