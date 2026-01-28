<?php
function foo(?string &$i) : void {}
function bar(?string &$i) : void {}

$c = null;

do {
    if ($c === null || $c === "" || $c === "0") {
        foo($c);
    } else {
        bar($c);
    }
} while (rand(0, 1));
