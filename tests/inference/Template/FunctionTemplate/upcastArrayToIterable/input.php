<?php
/**
 * @template K
 * @template V
 * @param iterable<K,V> $collection
 * @return V
 * @psalm-suppress InvalidReturnType
 */
function first($collection) {}

$one = first([1,2,3]);