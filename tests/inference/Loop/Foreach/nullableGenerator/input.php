<?php
/** @return Generator<int,int|null> */
function gen() : Generator {
    yield null;
    yield 1;
}
$gen = gen();
$a = "";
foreach ($gen as $i) {
    $a = $i;
}
