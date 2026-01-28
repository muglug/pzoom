<?php
/**
 * @template T
 * @param array<T> ...$iterators
 * @return Generator<T>
 */
function joinBySplat(array ...$iterators): iterable {
    foreach ($iterators as $iter) {
        foreach ($iter as $value) {
            yield $value;
        }
    }
}

/**
 * @return Generator<int, array<int>>
 */
function genIters(): Generator {
    yield [1,2,3];
    yield [4,5,6];
}

$values = joinBySplat(...genIters());

foreach ($values as $value) {
    echo $value;
}