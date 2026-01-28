<?php

function foo(int $bar) : string {
    return (string) $bar;
}

foreach (range(1.4, 10) as $x) {
    foo($x);
}
