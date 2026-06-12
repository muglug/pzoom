<?php
$tick = 0;

test($tick + 1);

$tick++;

test($tick);

/**
 * @psalm-param positive-int $tickedTimes
 */
function test(int $tickedTimes): void {}
