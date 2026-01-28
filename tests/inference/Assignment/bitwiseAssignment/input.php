<?php
$x = 0;
$x |= (int) (rand(0, 2) !== 2);
$x |= 1;
/** @psalm-suppress RedundantCondition Psalm now knows this is always truthy */
if ($x) {
    echo $x;
}
