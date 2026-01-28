<?php
/**
 * @param  int  $start
 * @param  int  $limit
 * @param  int  $step
 * @return Generator<int>
 */
function xrange($start, $limit, $step = 1) {
    for ($i = $start; $i <= $limit; $i += $step) {
        yield $i;
    }
}

$a = null;

/*
 * Note that an array is never created or returned,
 * which saves memory.
 */
foreach (xrange(1, 9, 2) as $number) {
    $a = $number;
}
