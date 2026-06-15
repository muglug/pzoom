<?php
/**
 * @param  iterable<int, int> $iter
 */
function iterator(iterable $iter): void
{
    foreach ($iter as $val) {
        //
    }
}

iterator([1, 2, 3, 4]);
iterator(new SplFixedArray(5));
