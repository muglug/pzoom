<?php
function foo(?string &$s) : void {}

function bar() : void {
    foo($bar);
}
