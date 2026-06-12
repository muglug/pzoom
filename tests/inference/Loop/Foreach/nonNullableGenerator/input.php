<?php
    /** @return Generator<int,int> */
    function gen() : Generator {
        yield 1;
    }
    $gen = gen();
    $a = "";
    foreach ($gen as $i) {
$a = $i;
    }
