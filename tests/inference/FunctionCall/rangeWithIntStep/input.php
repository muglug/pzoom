<?php

function foo(int $bar) : string {
    return (string) $bar;
}

foreach (range(1, 10, 1) as $x) {
    foo($x);
}
