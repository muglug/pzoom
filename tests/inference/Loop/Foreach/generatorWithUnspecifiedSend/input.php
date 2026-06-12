<?php
/** @return Generator<int,int> */
function gen() : Generator {
    return yield 1;
}
$gen = gen();
foreach ($gen as $i) {}
