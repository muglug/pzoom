<?php

function foo(string $bar) : void {}

foreach (range("a", "z") as $x) {
    foo($x);
}
