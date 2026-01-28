<?php
/** @var list<int> */
$leftCount = [1, 2, 3];
assert (count($leftCount) <= 0);
/** @var list<int> */
$rightCount = [1, 2, 3];
assert (0 >= count($rightCount));
