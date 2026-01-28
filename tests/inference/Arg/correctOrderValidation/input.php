<?php
function getString(int $i) : string {
    return rand(0, 1) ? "hello" : "";
}

function takesInt(int $i) : void {}

$i = rand(0, 10);

if (!($i = getString($i))) {}
