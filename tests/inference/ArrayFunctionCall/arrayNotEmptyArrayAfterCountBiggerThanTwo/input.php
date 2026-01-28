<?php
/** @var list<int> */
$leftCount = [1, 2, 3];
if (count($leftCount) > 2) {
    echo $leftCount[0];
}
/** @var list<int> */
$rightCount = [1, 2, 3];
if (2 < count($rightCount)) {
    echo $rightCount[0];
}
