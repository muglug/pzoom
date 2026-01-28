<?php
/** @psalm-var 1|2|3 $i */
$i = rand(1, 3);

/** @psalm-suppress ParadoxicalCondition */
switch($i) {
    case 1: break;
    case 2: break;
    case 3: break;
    default:
        echo "bar";
}
