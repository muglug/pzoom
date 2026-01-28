<?php
function foo(string $s) : void {
    @list(, $port) = explode(":", $s);
    echo isset($port) ? "cool" : "uncool";
}