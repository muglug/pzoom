<?php
function foo(?string &$i) : void {}
function bar(?string &$i) : void {}

$c = null;

while (rand(0, 1)) {
    if ($c === null || $c === "" || $c === "0") {
        foo($c);
    } else {
        bar($c);
    }
}
