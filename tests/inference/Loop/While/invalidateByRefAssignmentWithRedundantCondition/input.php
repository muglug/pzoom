<?php
function foo(?string $i) : void {}
function bar(?string $i) : void {}

$c = null;

while (rand(0, 1)) {
    if (!$c) {
        foo($c);
    } else {
        bar($c);
    }
}
